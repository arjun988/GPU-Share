//! libcudart-style stub: talks JSON over TCP to `gpumesh cuda bridge`.
//!
//! Set `GPUMESH_CUDA_BRIDGE=127.0.0.1:17999` (default) then:
//! `LD_LIBRARY_PATH=target/debug ./your_cuda_app` (Linux) so this `libcudart.so` is found.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

static BRIDGE: OnceLock<Mutex<Option<TcpStream>>> = OnceLock::new();

#[derive(Serialize)]
struct BridgeReq {
    op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ptr: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dst: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    src: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct BridgeResp {
    ok: bool,
    error: Option<String>,
    ptr: Option<u64>,
    data: Option<Vec<u8>>,
    device_count: Option<u32>,
}

fn stream() -> std::io::Result<std::sync::MutexGuard<'static, Option<TcpStream>>> {
    let slot = BRIDGE.get_or_init(|| Mutex::new(None));
    let mut g = slot.lock().unwrap();
    if g.is_none() {
        let addr = std::env::var("GPUMESH_CUDA_BRIDGE")
            .unwrap_or_else(|_| "127.0.0.1:17999".into());
        let s = TcpStream::connect(addr)?;
        s.set_nodelay(true)?;
        *g = Some(s);
    }
    Ok(g)
}

fn rpc(req: &BridgeReq) -> Result<BridgeResp, String> {
    let mut g = stream().map_err(|e| e.to_string())?;
    let sock = g.as_mut().ok_or("no bridge")?;
    let body = serde_json::to_vec(req).map_err(|e| e.to_string())?;
    sock.write_all(&(body.len() as u32).to_be_bytes())
        .map_err(|e| e.to_string())?;
    sock.write_all(&body).map_err(|e| e.to_string())?;
    sock.flush().map_err(|e| e.to_string())?;
    let mut lenb = [0u8; 4];
    sock.read_exact(&mut lenb).map_err(|e| e.to_string())?;
    let n = u32::from_be_bytes(lenb) as usize;
    if n > 64 * 1024 * 1024 {
        return Err("bridge frame too large".into());
    }
    let mut buf = vec![0u8; n];
    sock.read_exact(&mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

fn cuda_err(msg: &str) -> i32 {
    eprintln!("gpumesh cudart stub: {msg}");
    1
}

#[no_mangle]
pub extern "C" fn cudaGetDeviceCount(count: *mut i32) -> i32 {
    if count.is_null() {
        return cuda_err("null");
    }
    match rpc(&BridgeReq {
        op: "device_count".into(),
        bytes: None,
        ptr: None,
        dst: None,
        src: None,
        n: None,
        data: None,
    }) {
        Ok(r) if r.ok => {
            unsafe {
                *count = r.device_count.unwrap_or(0) as i32;
            }
            0
        }
        Ok(r) => cuda_err(r.error.as_deref().unwrap_or("device_count failed")),
        Err(e) => cuda_err(&e),
    }
}

#[no_mangle]
pub extern "C" fn cudaMalloc(dev_ptr: *mut *mut std::ffi::c_void, size: usize) -> i32 {
    if dev_ptr.is_null() {
        return cuda_err("null");
    }
    match rpc(&BridgeReq {
        op: "malloc".into(),
        bytes: Some(size as u64),
        ptr: None,
        dst: None,
        src: None,
        n: None,
        data: None,
    }) {
        Ok(r) if r.ok => {
            unsafe {
                *dev_ptr = r.ptr.unwrap_or(0) as *mut std::ffi::c_void;
            }
            0
        }
        Ok(r) => cuda_err(r.error.as_deref().unwrap_or("malloc failed")),
        Err(e) => cuda_err(&e),
    }
}

#[no_mangle]
pub extern "C" fn cudaFree(dev_ptr: *mut std::ffi::c_void) -> i32 {
    match rpc(&BridgeReq {
        op: "free".into(),
        bytes: None,
        ptr: Some(dev_ptr as u64),
        dst: None,
        src: None,
        n: None,
        data: None,
    }) {
        Ok(r) if r.ok => 0,
        Ok(r) => cuda_err(r.error.as_deref().unwrap_or("free failed")),
        Err(e) => cuda_err(&e),
    }
}

/// cudaMemcpyKind: 1=HtoD, 2=DtoH (matches CUDA runtime enum-ish: cudaMemcpyHostToDevice=1)
#[no_mangle]
pub extern "C" fn cudaMemcpy(
    dst: *mut std::ffi::c_void,
    src: *const std::ffi::c_void,
    count: usize,
    kind: i32,
) -> i32 {
    match kind {
        1 => {
            // HtoD
            let data = unsafe { std::slice::from_raw_parts(src as *const u8, count) }.to_vec();
            match rpc(&BridgeReq {
                op: "htod".into(),
                bytes: None,
                ptr: None,
                dst: Some(dst as u64),
                src: None,
                n: None,
                data: Some(data),
            }) {
                Ok(r) if r.ok => 0,
                Ok(r) => cuda_err(r.error.as_deref().unwrap_or("htod failed")),
                Err(e) => cuda_err(&e),
            }
        }
        2 => {
            match rpc(&BridgeReq {
                op: "dtoh".into(),
                bytes: Some(count as u64),
                ptr: None,
                dst: None,
                src: Some(src as u64),
                n: None,
                data: None,
            }) {
                Ok(r) if r.ok => {
                    let data = r.data.unwrap_or_default();
                    if data.len() != count {
                        return cuda_err("dtoh size mismatch");
                    }
                    unsafe {
                        std::ptr::copy_nonoverlapping(data.as_ptr(), dst as *mut u8, count);
                    }
                    0
                }
                Ok(r) => cuda_err(r.error.as_deref().unwrap_or("dtoh failed")),
                Err(e) => cuda_err(&e),
            }
        }
        _ => cuda_err("unsupported cudaMemcpy kind (use 1=HtoD, 2=DtoH)"),
    }
}

#[no_mangle]
pub extern "C" fn cudaDeviceSynchronize() -> i32 {
    match rpc(&BridgeReq {
        op: "sync".into(),
        bytes: None,
        ptr: None,
        dst: None,
        src: None,
        n: None,
        data: None,
    }) {
        Ok(r) if r.ok => 0,
        Ok(r) => cuda_err(r.error.as_deref().unwrap_or("sync failed")),
        Err(e) => cuda_err(&e),
    }
}

#[no_mangle]
pub extern "C" fn gpumeshVectorAddF32(a: *mut std::ffi::c_void, b: *mut std::ffi::c_void, out: *mut std::ffi::c_void, n: u32) -> i32 {
    match rpc(&BridgeReq {
        op: "vector_add_f32".into(),
        bytes: None,
        ptr: Some(out as u64),
        dst: Some(a as u64),
        src: Some(b as u64),
        n: Some(n),
        data: None,
    }) {
        Ok(r) if r.ok => 0,
        Ok(r) => cuda_err(r.error.as_deref().unwrap_or("vector_add failed")),
        Err(e) => cuda_err(&e),
    }
}

#[no_mangle]
pub extern "C" fn cudaGetErrorString(err: i32) -> *const std::ffi::c_char {
    if err == 0 {
        b"no error\0".as_ptr() as *const std::ffi::c_char
    } else {
        b"gpumesh remoting error\0".as_ptr() as *const std::ffi::c_char
    }
}
