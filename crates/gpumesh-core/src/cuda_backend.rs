//! CUDA remoting device backends (R3).
//!
//! - `host-memory`: buffers in host RAM (fallback)
//! - `cuda-driver`: real NVIDIA driver via cudarc + NVRTC (`cuda-driver` feature)

use std::collections::HashMap;
use std::time::Instant;

use gpumesh_protocol::{CudaDeviceInfo, CudaOpKind};

pub const MAX_SINGLE_ALLOC: u64 = 256 * 1024 * 1024;
pub const MAX_MEMCPY: u64 = 64 * 1024 * 1024;
pub const MAX_PTX_BYTES: usize = 256 * 1024;
pub const MAX_KERNEL_ARGS: usize = 16;

#[derive(Default)]
pub struct CudaPartial {
    pub device_count: Option<u32>,
    pub device: Option<CudaDeviceInfo>,
    pub ptr: Option<u64>,
    pub data: Option<Vec<u8>>,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub device_index: Option<u32>,
    pub event_id: Option<u64>,
    pub module_id: Option<u64>,
    pub elapsed_ms: Option<f32>,
}

pub struct BackendSession {
    pub backend_name: String,
    devices: Vec<CudaDeviceInfo>,
    max_alloc: u64,
    used: u64,
    next_ptr: u64,
    next_event: u64,
    next_module: u64,
    current_device: u32,
    events: HashMap<u64, Instant>,
    host_buffers: HashMap<u64, Vec<u8>>,
    #[cfg(feature = "cuda-driver")]
    cuda: Option<CudaState>,
}

#[cfg(feature = "cuda-driver")]
struct CudaState {
    ctx: std::sync::Arc<cudarc::driver::CudaContext>,
    stream: std::sync::Arc<cudarc::driver::CudaStream>,
    buffers: HashMap<u64, cudarc::driver::CudaSlice<u8>>,
    modules: HashMap<u64, std::sync::Arc<cudarc::driver::CudaModule>>,
    vector_add: cudarc::driver::CudaFunction,
}

impl BackendSession {
    pub fn open(max_alloc: u64, devices: Vec<CudaDeviceInfo>) -> Self {
        #[cfg(feature = "cuda-driver")]
        {
            match CudaState::try_new() {
                Ok(cuda) => {
                    tracing::info!("CUDA remoting backend: cuda-driver");
                    return Self {
                        backend_name: "cuda-driver".into(),
                        devices,
                        max_alloc,
                        used: 0,
                        next_ptr: 0x1000,
                        next_event: 1,
                        next_module: 1,
                        current_device: 0,
                        events: HashMap::new(),
                        host_buffers: HashMap::new(),
                        cuda: Some(cuda),
                    };
                }
                Err(e) => {
                    tracing::warn!("cuda-driver unavailable ({e}); using host-memory backend");
                }
            }
        }

        Self {
            backend_name: "host-memory".into(),
            devices,
            max_alloc,
            used: 0,
            next_ptr: 0x1000,
            next_event: 1,
            next_module: 1,
            current_device: 0,
            events: HashMap::new(),
            host_buffers: HashMap::new(),
            #[cfg(feature = "cuda-driver")]
            cuda: None,
        }
    }

    #[allow(dead_code)]
    pub fn is_cuda_driver(&self) -> bool {
        #[cfg(feature = "cuda-driver")]
        {
            return self.cuda.is_some();
        }
        #[allow(unreachable_code)]
        false
    }

    pub fn exec(&mut self, op: CudaOpKind) -> (bool, Option<String>, CudaPartial) {
        let mut partial = CudaPartial::default();
        match op {
            CudaOpKind::DeviceCount => {
                partial.device_count = Some(self.devices.len() as u32);
                (true, None, partial)
            }
            CudaOpKind::DeviceProps { device } => match self.devices.get(device as usize) {
                Some(d) => {
                    partial.device = Some(d.clone());
                    (true, None, partial)
                }
                None => (false, Some(format!("invalid device index {device}")), partial),
            },
            CudaOpKind::GetDevice => {
                partial.device_index = Some(self.current_device);
                (true, None, partial)
            }
            CudaOpKind::SetDevice { device } => {
                if (device as usize) >= self.devices.len() {
                    return (false, Some(format!("invalid device {device}")), partial);
                }
                self.current_device = device;
                partial.device_index = Some(device);
                (true, None, partial)
            }
            CudaOpKind::MemGetInfo => {
                let total = self
                    .devices
                    .get(self.current_device as usize)
                    .map(|d| d.vram_total_mb.saturating_mul(1024 * 1024))
                    .unwrap_or(self.max_alloc);
                let free = self.max_alloc.saturating_sub(self.used).min(total);
                partial.free_bytes = Some(free);
                partial.total_bytes = Some(total);
                (true, None, partial)
            }
            CudaOpKind::EventCreate => {
                let id = self.next_event;
                self.next_event = self.next_event.saturating_add(1);
                self.events.insert(id, Instant::now());
                partial.event_id = Some(id);
                (true, None, partial)
            }
            CudaOpKind::EventDestroy { event } => {
                if self.events.remove(&event).is_some() {
                    (true, None, partial)
                } else {
                    (false, Some(format!("invalid event {event}")), partial)
                }
            }
            CudaOpKind::EventRecord { event } => {
                if let Some(e) = self.events.get_mut(&event) {
                    *e = Instant::now();
                    (true, None, partial)
                } else {
                    (false, Some(format!("invalid event {event}")), partial)
                }
            }
            CudaOpKind::EventElapsed { start, end } => {
                match (self.events.get(&start), self.events.get(&end)) {
                    (Some(s), Some(e)) => {
                        let ms = e.saturating_duration_since(*s).as_secs_f32() * 1000.0;
                        partial.elapsed_ms = Some(ms);
                        (true, None, partial)
                    }
                    _ => (false, Some("invalid event id(s)".into()), partial),
                }
            }
            CudaOpKind::Sync => self.op_sync(),
            CudaOpKind::Malloc { bytes } => self.op_malloc(bytes),
            CudaOpKind::Free { ptr } => self.op_free(ptr),
            CudaOpKind::MemcpyHtoD { dst, data } => self.op_htod(dst, data),
            CudaOpKind::MemcpyDtoH { src, bytes } => self.op_dtoh(src, bytes),
            CudaOpKind::MemcpyDtoD { dst, src, bytes } => self.op_dtod(dst, src, bytes),
            CudaOpKind::Memset { ptr, value, bytes } => self.op_memset(ptr, value, bytes),
            CudaOpKind::VectorAddF32 { a, b, out, n } => self.op_vector_add(a, b, out, n),
            CudaOpKind::LoadModulePtx { ptx } => self.op_load_ptx(ptx),
            CudaOpKind::LaunchKernel {
                module_id,
                function,
                grid,
                block,
                shared_mem,
                args,
            } => self.op_launch(module_id, function, grid, block, shared_mem, args),
        }
    }

    fn check_alloc(&self, bytes: u64) -> Result<(), String> {
        if bytes == 0 {
            return Err("cudaMalloc size 0".into());
        }
        if bytes > MAX_SINGLE_ALLOC {
            return Err(format!(
                "alloc {bytes} exceeds per-buffer cap {MAX_SINGLE_ALLOC}"
            ));
        }
        if self.used.saturating_add(bytes) > self.max_alloc {
            return Err(format!(
                "session alloc cap exceeded (used {} + {bytes} > {})",
                self.used, self.max_alloc
            ));
        }
        Ok(())
    }

    fn op_sync(&mut self) -> (bool, Option<String>, CudaPartial) {
        #[cfg(feature = "cuda-driver")]
        if let Some(cuda) = &self.cuda {
            if let Err(e) = cuda.stream.synchronize() {
                return (false, Some(format!("cuda synchronize: {e}")), CudaPartial::default());
            }
        }
        (true, None, CudaPartial::default())
    }

    fn op_malloc(&mut self, bytes: u64) -> (bool, Option<String>, CudaPartial) {
        if let Err(e) = self.check_alloc(bytes) {
            return (false, Some(e), CudaPartial::default());
        }
        let ptr = self.next_ptr;
        self.next_ptr = self.next_ptr.saturating_add(bytes.max(64)).saturating_add(64);

        #[cfg(feature = "cuda-driver")]
        if let Some(cuda) = &mut self.cuda {
            match cuda.stream.alloc_zeros::<u8>(bytes as usize) {
                Ok(slice) => {
                    cuda.buffers.insert(ptr, slice);
                    self.used = self.used.saturating_add(bytes);
                    return (
                        true,
                        None,
                        CudaPartial {
                            ptr: Some(ptr),
                            ..Default::default()
                        },
                    );
                }
                Err(e) => return (false, Some(format!("cudaMalloc: {e}")), CudaPartial::default()),
            }
        }

        self.host_buffers.insert(ptr, vec![0u8; bytes as usize]);
        self.used = self.used.saturating_add(bytes);
        (
            true,
            None,
            CudaPartial {
                ptr: Some(ptr),
                ..Default::default()
            },
        )
    }

    fn op_free(&mut self, ptr: u64) -> (bool, Option<String>, CudaPartial) {
        #[cfg(feature = "cuda-driver")]
        if let Some(cuda) = &mut self.cuda {
            return if let Some(buf) = cuda.buffers.remove(&ptr) {
                self.used = self.used.saturating_sub(buf.len() as u64);
                (true, None, CudaPartial::default())
            } else {
                (
                    false,
                    Some(format!("invalid device ptr {ptr:#x}")),
                    CudaPartial::default(),
                )
            };
        }

        if let Some(buf) = self.host_buffers.remove(&ptr) {
            self.used = self.used.saturating_sub(buf.len() as u64);
            (true, None, CudaPartial::default())
        } else {
            (
                false,
                Some(format!("invalid device ptr {ptr:#x}")),
                CudaPartial::default(),
            )
        }
    }

    fn read_buf(&self, ptr: u64) -> Result<Vec<u8>, String> {
        #[cfg(feature = "cuda-driver")]
        if let Some(cuda) = &self.cuda {
            let buf = cuda
                .buffers
                .get(&ptr)
                .ok_or_else(|| format!("invalid ptr {ptr:#x}"))?;
            return cuda.stream.clone_dtoh(buf).map_err(|e| e.to_string());
        }
        self.host_buffers
            .get(&ptr)
            .cloned()
            .ok_or_else(|| format!("invalid ptr {ptr:#x}"))
    }

    fn write_buf(&mut self, ptr: u64, data: &[u8]) -> Result<(), String> {
        #[cfg(feature = "cuda-driver")]
        if let Some(cuda) = &mut self.cuda {
            let buf = cuda
                .buffers
                .get_mut(&ptr)
                .ok_or_else(|| format!("invalid ptr {ptr:#x}"))?;
            if data.len() != buf.len() {
                return Err("buffer size mismatch on write".into());
            }
            return cuda.stream.memcpy_htod(data, buf).map_err(|e| e.to_string());
        }
        let buf = self
            .host_buffers
            .get_mut(&ptr)
            .ok_or_else(|| format!("invalid ptr {ptr:#x}"))?;
        if data.len() != buf.len() {
            return Err("buffer size mismatch on write".into());
        }
        buf.copy_from_slice(data);
        Ok(())
    }

    fn buf_len(&self, ptr: u64) -> Result<usize, String> {
        #[cfg(feature = "cuda-driver")]
        if let Some(cuda) = &self.cuda {
            return cuda
                .buffers
                .get(&ptr)
                .map(|b| b.len())
                .ok_or_else(|| format!("invalid ptr {ptr:#x}"));
        }
        self.host_buffers
            .get(&ptr)
            .map(|b| b.len())
            .ok_or_else(|| format!("invalid ptr {ptr:#x}"))
    }

    fn op_htod(&mut self, dst: u64, data: Vec<u8>) -> (bool, Option<String>, CudaPartial) {
        if data.len() as u64 > MAX_MEMCPY {
            return (false, Some("memcpy too large".into()), CudaPartial::default());
        }
        match self.read_buf(dst) {
            Ok(mut full) => {
                if data.len() > full.len() {
                    return (false, Some("HtoD overflows buffer".into()), CudaPartial::default());
                }
                full[..data.len()].copy_from_slice(&data);
                match self.write_buf(dst, &full) {
                    Ok(()) => (true, None, CudaPartial::default()),
                    Err(e) => (false, Some(e), CudaPartial::default()),
                }
            }
            Err(e) => (false, Some(e), CudaPartial::default()),
        }
    }

    fn op_dtoh(&mut self, src: u64, bytes: u64) -> (bool, Option<String>, CudaPartial) {
        if bytes > MAX_MEMCPY {
            return (false, Some("memcpy too large".into()), CudaPartial::default());
        }
        match self.read_buf(src) {
            Ok(full) if (bytes as usize) <= full.len() => (
                true,
                None,
                CudaPartial {
                    data: Some(full[..bytes as usize].to_vec()),
                    ..Default::default()
                },
            ),
            Ok(_) => (false, Some("DtoH overflows buffer".into()), CudaPartial::default()),
            Err(e) => (false, Some(e), CudaPartial::default()),
        }
    }

    fn op_dtod(&mut self, dst: u64, src: u64, bytes: u64) -> (bool, Option<String>, CudaPartial) {
        let (ok, err, mut p) = self.op_dtoh(src, bytes);
        if !ok {
            return (ok, err, p);
        }
        let data = p.data.take().unwrap_or_default();
        self.op_htod(dst, data)
    }

    fn op_memset(
        &mut self,
        ptr: u64,
        value: u8,
        bytes: u64,
    ) -> (bool, Option<String>, CudaPartial) {
        match self.read_buf(ptr) {
            Ok(mut full) => {
                if (bytes as usize) > full.len() {
                    return (false, Some("memset overflows buffer".into()), CudaPartial::default());
                }
                full[..bytes as usize].fill(value);
                match self.write_buf(ptr, &full) {
                    Ok(()) => (true, None, CudaPartial::default()),
                    Err(e) => (false, Some(e), CudaPartial::default()),
                }
            }
            Err(e) => (false, Some(e), CudaPartial::default()),
        }
    }

    fn op_vector_add(
        &mut self,
        a: u64,
        b: u64,
        out: u64,
        n: u32,
    ) -> (bool, Option<String>, CudaPartial) {
        let need = (n as usize).saturating_mul(4);
        for p in [a, b, out] {
            match self.buf_len(p) {
                Ok(len) if len >= need => {}
                Ok(_) => {
                    return (
                        false,
                        Some("vector_add buffer too small".into()),
                        CudaPartial::default(),
                    )
                }
                Err(e) => return (false, Some(e), CudaPartial::default()),
            }
        }

        #[cfg(feature = "cuda-driver")]
        if let Some(cuda) = &self.cuda {
            let a_host = match self.read_buf(a) {
                Ok(v) => v,
                Err(e) => return (false, Some(e), CudaPartial::default()),
            };
            let b_host = match self.read_buf(b) {
                Ok(v) => v,
                Err(e) => return (false, Some(e), CudaPartial::default()),
            };
            let a_f: Vec<f32> = a_host[..need]
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                .collect();
            let b_f: Vec<f32> = b_host[..need]
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                .collect();
            let stream = cuda.stream.clone();
            let f = cuda.vector_add.clone();
            let a_dev = match stream.clone_htod(&a_f) {
                Ok(v) => v,
                Err(e) => return (false, Some(e.to_string()), CudaPartial::default()),
            };
            let b_dev = match stream.clone_htod(&b_f) {
                Ok(v) => v,
                Err(e) => return (false, Some(e.to_string()), CudaPartial::default()),
            };
            let mut out_dev = match stream.alloc_zeros::<f32>(n as usize) {
                Ok(v) => v,
                Err(e) => return (false, Some(e.to_string()), CudaPartial::default()),
            };
            let n_i = n as i32;
            let cfg = cudarc::driver::LaunchConfig::for_num_elems(n);
            use cudarc::driver::PushKernelArg;
            let mut builder = stream.launch_builder(&f);
            builder.arg(&mut out_dev).arg(&a_dev).arg(&b_dev).arg(&n_i);
            if let Err(e) = unsafe { builder.launch(cfg) } {
                return (
                    false,
                    Some(format!("vector_add launch: {e}")),
                    CudaPartial::default(),
                );
            }
            if let Err(e) = stream.synchronize() {
                return (false, Some(format!("vector_add sync: {e}")), CudaPartial::default());
            }
            let out_f: Vec<f32> = match stream.clone_dtoh(&out_dev) {
                Ok(v) => v,
                Err(e) => return (false, Some(e.to_string()), CudaPartial::default()),
            };
            let mut out_bytes = match self.read_buf(out) {
                Ok(v) => v,
                Err(e) => return (false, Some(e), CudaPartial::default()),
            };
            for (i, v) in out_f.iter().enumerate() {
                out_bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            return match self.write_buf(out, &out_bytes) {
                Ok(()) => (true, None, CudaPartial::default()),
                Err(e) => (false, Some(e), CudaPartial::default()),
            };
        }

        let ab = match self.read_buf(a) {
            Ok(v) => v,
            Err(e) => return (false, Some(e), CudaPartial::default()),
        };
        let bb = match self.read_buf(b) {
            Ok(v) => v,
            Err(e) => return (false, Some(e), CudaPartial::default()),
        };
        let mut ob = match self.read_buf(out) {
            Ok(v) => v,
            Err(e) => return (false, Some(e), CudaPartial::default()),
        };
        for i in 0..n as usize {
            let o = i * 4;
            let x = f32::from_le_bytes(ab[o..o + 4].try_into().unwrap());
            let y = f32::from_le_bytes(bb[o..o + 4].try_into().unwrap());
            ob[o..o + 4].copy_from_slice(&(x + y).to_le_bytes());
        }
        match self.write_buf(out, &ob) {
            Ok(()) => (true, None, CudaPartial::default()),
            Err(e) => (false, Some(e), CudaPartial::default()),
        }
    }

    fn op_load_ptx(&mut self, ptx: String) -> (bool, Option<String>, CudaPartial) {
        if ptx.len() > MAX_PTX_BYTES {
            return (
                false,
                Some(format!("PTX exceeds {MAX_PTX_BYTES} bytes")),
                CudaPartial::default(),
            );
        }
        #[cfg(feature = "cuda-driver")]
        if let Some(cuda) = &mut self.cuda {
            let compiled = match cudarc::nvrtc::compile_ptx(&ptx) {
                Ok(p) => p,
                Err(e) => {
                    return (
                        false,
                        Some(format!("NVRTC compile: {e}")),
                        CudaPartial::default(),
                    )
                }
            };
            let module = match cuda.ctx.load_module(compiled) {
                Ok(m) => m,
                Err(e) => {
                    return (false, Some(format!("load module: {e}")), CudaPartial::default())
                }
            };
            let id = self.next_module;
            self.next_module = self.next_module.saturating_add(1);
            cuda.modules.insert(id, module);
            return (
                true,
                None,
                CudaPartial {
                    module_id: Some(id),
                    ..Default::default()
                },
            );
        }
        (
            false,
            Some("LoadModulePtx requires cuda-driver backend".into()),
            CudaPartial::default(),
        )
    }

    fn op_launch(
        &mut self,
        module_id: u64,
        function: String,
        grid: [u32; 3],
        block: [u32; 3],
        shared_mem: u32,
        args: Vec<u64>,
    ) -> (bool, Option<String>, CudaPartial) {
        if args.len() > MAX_KERNEL_ARGS {
            return (
                false,
                Some(format!("too many kernel args (max {MAX_KERNEL_ARGS})")),
                CudaPartial::default(),
            );
        }
        if function.is_empty()
            || function.len() > 128
            || !function
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return (false, Some("invalid function name".into()), CudaPartial::default());
        }

        #[cfg(feature = "cuda-driver")]
        if let Some(cuda) = &self.cuda {
            let Some(module) = cuda.modules.get(&module_id).cloned() else {
                return (
                    false,
                    Some(format!("unknown module {module_id}")),
                    CudaPartial::default(),
                );
            };
            let func = match module.load_function(&function) {
                Ok(f) => f,
                Err(e) => {
                    return (
                        false,
                        Some(format!("load function: {e}")),
                        CudaPartial::default(),
                    )
                }
            };
            use cudarc::driver::DevicePtr;
            let mut ptr_vals: Vec<cudarc::driver::sys::CUdeviceptr> = Vec::with_capacity(args.len());
            let mut _guards = Vec::new();
            for h in &args {
                let Some(buf) = cuda.buffers.get(h) else {
                    return (
                        false,
                        Some(format!("invalid kernel arg ptr {h:#x}")),
                        CudaPartial::default(),
                    );
                };
                let (ptr, guard) = buf.device_ptr(&cuda.stream);
                ptr_vals.push(ptr);
                _guards.push(guard);
            }
            let cfg = cudarc::driver::LaunchConfig {
                grid_dim: (grid[0].max(1), grid[1].max(1), grid[2].max(1)),
                block_dim: (block[0].max(1), block[1].max(1), block[2].max(1)),
                shared_mem_bytes: shared_mem,
            };
            let stream = cuda.stream.clone();
            // Manual launch for 0..=8 pointer args (covers the spike).
            let launch_res = unsafe { launch_ptr_kernel(&stream, &func, cfg, &ptr_vals) };
            return match launch_res {
                Ok(()) => {
                    if let Err(e) = stream.synchronize() {
                        (
                            false,
                            Some(format!("kernel sync: {e}")),
                            CudaPartial::default(),
                        )
                    } else {
                        (true, None, CudaPartial::default())
                    }
                }
                Err(e) => (false, Some(e), CudaPartial::default()),
            };
        }

        let _ = (module_id, function, grid, block, shared_mem, args);
        (
            false,
            Some("LaunchKernel requires cuda-driver backend".into()),
            CudaPartial::default(),
        )
    }
}

#[cfg(feature = "cuda-driver")]
impl CudaState {
    fn try_new() -> Result<Self, String> {
        let ctx = cudarc::driver::CudaContext::new(0).map_err(|e| e.to_string())?;
        let stream = ctx.default_stream();
        let ptx = cudarc::nvrtc::compile_ptx(
            r#"
extern "C" __global__ void gpumesh_vector_add_f32(float* out, const float* a, const float* b, int n) {
  int i = blockIdx.x * blockDim.x + threadIdx.x;
  if (i < n) out[i] = a[i] + b[i];
}
"#,
        )
        .map_err(|e| e.to_string())?;
        let module = ctx.load_module(ptx).map_err(|e| e.to_string())?;
        let vector_add = module
            .load_function("gpumesh_vector_add_f32")
            .map_err(|e| e.to_string())?;
        Ok(Self {
            ctx,
            stream,
            buffers: HashMap::new(),
            modules: HashMap::new(),
            vector_add,
        })
    }
}

#[cfg(feature = "cuda-driver")]
unsafe fn launch_ptr_kernel(
    stream: &cudarc::driver::CudaStream,
    func: &cudarc::driver::CudaFunction,
    cfg: cudarc::driver::LaunchConfig,
    ptrs: &[cudarc::driver::sys::CUdeviceptr],
) -> Result<(), String> {
    use cudarc::driver::PushKernelArg;
    let map = |r: Result<Option<(cudarc::driver::CudaEvent, cudarc::driver::CudaEvent)>, _>| {
        r.map(|_| ()).map_err(|e: cudarc::driver::DriverError| e.to_string())
    };
    match ptrs.len() {
        0 => {
            let mut b = stream.launch_builder(func);
            map(unsafe { b.launch(cfg) })
        }
        1 => {
            let mut b = stream.launch_builder(func);
            b.arg(&ptrs[0]);
            map(unsafe { b.launch(cfg) })
        }
        2 => {
            let mut b = stream.launch_builder(func);
            b.arg(&ptrs[0]).arg(&ptrs[1]);
            map(unsafe { b.launch(cfg) })
        }
        3 => {
            let mut b = stream.launch_builder(func);
            b.arg(&ptrs[0]).arg(&ptrs[1]).arg(&ptrs[2]);
            map(unsafe { b.launch(cfg) })
        }
        4 => {
            let mut b = stream.launch_builder(func);
            b.arg(&ptrs[0]).arg(&ptrs[1]).arg(&ptrs[2]).arg(&ptrs[3]);
            map(unsafe { b.launch(cfg) })
        }
        n => Err(format!(
            "LaunchKernel supports at most 4 pointer args in this spike (got {n})"
        )),
    }
}

/// Probe whether the real CUDA driver can be loaded on this host.
pub fn probe_cuda_driver() -> Result<String, String> {
    #[cfg(feature = "cuda-driver")]
    {
        let ctx = cudarc::driver::CudaContext::new(0).map_err(|e| e.to_string())?;
        let _ = ctx.default_stream();
        Ok("cuda-driver (cudarc + NVRTC)".into())
    }
    #[cfg(not(feature = "cuda-driver"))]
    Err("built without cuda-driver feature".into())
}
