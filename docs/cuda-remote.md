# CUDA remoting (R2 + R3)

**Status:** LAN remoting shipped as `gpumesh cuda`.  
**R3:** real **cuda-driver** backend (cudarc + NVRTC PTX) when `libcuda` loads; otherwise **host-memory** fallback.  
**Not:** a drop-in CUDA ICD for arbitrary PyTorch / games / D3D.

Related: [research.md](./research.md) · [gpu-desktop.md](./gpu-desktop.md)

---

## What this is

Authenticated CUDA Runtime-style remoting over GPUMesh pairing + QUIC.

| Piece | Behavior |
| --- | --- |
| Capability | `gpu_remote_allowed` — separate from jobs and desktop |
| Host | `gpumesh cuda share` |
| Client demo | `gpumesh cuda demo --peer <host>` |
| C apps | `gpumesh cuda bridge` + `gpumesh-cudart-stub` |
| Ops | device query, malloc/free, memcpy H↔D / D↔D, memset, sync, meminfo, events, load PTX, launch kernel (≤4 ptr args), built-in `vector_add_f32` |
| Backend | **`cuda-driver`** (real GPU kernels) or **`host-memory`** fallback |

---

## Quick start

### Host (GPU)

```bash
gpumesh cuda doctor          # should show cuda-driver when NVIDIA driver loads
gpumesh cuda share
gpumesh cuda allow bob-laptop
```

### Client (Rust demo)

```bash
gpumesh cuda demo --peer alice-pc
gpumesh cuda bench --peer alice-pc --iters 50
```

Demo reports **Backend: cuda-driver** when the host ran real PTX `vector_add`.

### Client (C stub)

```bash
# terminal 1
gpumesh cuda bridge --peer alice-pc --bind 127.0.0.1:17999

# terminal 2
cargo build -p gpumesh-cudart-stub
export GPUMESH_CUDA_BRIDGE=127.0.0.1:17999
gcc -O2 examples/cuda_stub_sample.c -L target/debug -lcudart -o /tmp/gm-cuda-sample
LD_LIBRARY_PATH=target/debug /tmp/gm-cuda-sample
```

---

## CLI

```bash
gpumesh cuda doctor
gpumesh cuda share
gpumesh cuda allow <peer>
gpumesh cuda demo --peer <peer> [--n 262144]
gpumesh cuda bench --peer <peer> [--iters 50]
gpumesh cuda bridge --peer <peer> [--bind 127.0.0.1:17999]
```

Protocol minor: **1.4** (`client_ver` 2).

---

## Security

| Rule | Behavior |
| --- | --- |
| CUDA remoting allow | Does **not** grant jobs |
| Alloc / PTX caps | Per-session alloc, 256 KiB PTX, 4 kernel pointer args |
| User PTX | Only in this remoting session; still trusted-peer model |

---

## OpenGL

**Deferred.** `GpuRemoteOpen { api: "opengl" }` is rejected. Use `gpumesh desktop` for GUI apps.

---

## Limits

| Item | Notes |
| --- | --- |
| PyTorch / full libcudart | Not covered — stub is a small ABI |
| WAN | Expected poor |
| Go | `cuda demo` verifies vector-add; backend should be `cuda-driver` on NVIDIA hosts |
