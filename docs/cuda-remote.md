# CUDA remoting (R2 spike)

**Status:** LAN-oriented spike shipped as `gpumesh cuda`.  
**Not:** a drop-in `libcuda` / ICD for arbitrary PyTorch, games, or GUI apps.

Related: [research.md](./research.md) · [gpu-desktop.md](./gpu-desktop.md) · hybrid [app](../README.md#hybrid-app-launcher-gpumesh-app)

---

## What this is

Authenticated **CUDA Runtime-style remoting** over GPUMesh pairing + QUIC:

| Piece | Behavior |
| --- | --- |
| Capability | `gpu_remote_allowed` — separate from jobs and desktop |
| Host | `gpumesh cuda share` |
| Client demo | `gpumesh cuda demo --peer <host>` |
| Ops | device query, malloc/free, memcpy H↔D, memset, sync, built-in `vector_add_f32` |
| Backend | `host-memory` — buffers live in **host RAM**; device **identity** from NVML |

Honest UX: your local process speaks the remoting API; compute/memory ops execute on the **peer host**. This is not “my local `.exe` loads peer CUDA as a local device.”

---

## Quick start (2 machines, same LAN)

### Host (GPU)

```bash
gpumesh init --name alice-pc
gpumesh cuda doctor
gpumesh cuda share          # leave running
# after pairing:
gpumesh cuda allow bob-laptop
```

### Client

```bash
gpumesh init --name bob-laptop
gpumesh pair '<alice-code>'
# mutual pair, then:
gpumesh cuda demo --peer alice-pc
gpumesh cuda bench --peer alice-pc --iters 50
```

---

## CLI

```bash
gpumesh cuda doctor
gpumesh cuda share
gpumesh cuda allow <peer>
gpumesh cuda demo --peer <peer> [--n 262144]
gpumesh cuda bench --peer <peer> [--iters 50]
```

Also under **CUDA remoting (R2)** in `gpumesh start`.

---

## Protocol (sketch)

1. Signed Hello (existing)  
2. `GpuRemoteOpen { api: "cuda", client_ver: 1 }`  
3. Host checks `cuda_remote_sharing` + `gpu_remote_allowed` + GPU present + util gates  
4. `GpuRemoteOffer` with devices + alloc cap  
5. `CudaOp` / `CudaResult` stream  
6. `GpuRemoteClose`

Protocol minor: **1.3**.

---

## Security

| Rule | Behavior |
| --- | --- |
| Job allow | Docker jobs only |
| Desktop allow | RDP/VNC tunnel (+ jobs for convenience) |
| CUDA remoting allow | **Only** remoting — does **not** grant jobs |
| Deny | Clears all three |
| Alloc caps | Per-session + per-buffer limits |

---

## Limits / go-no-go

| Item | Notes |
| --- | --- |
| Backend | Spike uses host-memory, not full device CUDA driver launch |
| PyTorch / arbitrary CUDA | **Out of scope** for R2 — needs much larger API surface + real libcuda interposition |
| WAN | Expected poor; doctor/demo warn LAN-only |
| Success metric | `cuda demo` verifies vector-add; `cuda bench` prints Sync / 4KiB HtoD p50/p99 |

**Go:** expand API + real CUDA driver backend (R3).  
**No-go:** keep hybrid `gpumesh app` + desktop for product UX.

---

## Compare modes

| Mode | Process location | GPU | Command |
| --- | --- | --- | --- |
| Jobs | Host (Docker) | Host | `gpumesh run` |
| App hybrid | Host (Docker) | Host | `gpumesh app` |
| Desktop | Host OS session | Host | `gpumesh desktop` |
| CUDA remoting | Client talks remoting API | Host ops | `gpumesh cuda` |
