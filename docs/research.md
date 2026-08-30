# Research: run *local* apps on a *remote* peer GPU

**Status:** Path B CUDA remoting is **not** built. **R1 hybrid launcher is shipped** (`gpumesh app`).  
**Related:** [gpu-desktop.md](./gpu-desktop.md) (remote desktop / RDP–VNC tunnel).

---

## 1. Problem statement

### What the user wants

> I am the **client**. I keep my apps installed **on my machine**. I only want **GPU access** from a peer. When I open Blender / Unity / a CUDA app / a game engine locally, the heavy GPU work should run on **their** GPU.

### What GPUMesh does today (contrast)

| Mode | Where the app process runs | Where the GPU runs | Status |
| --- | --- | --- | --- |
| **Jobs** (`gpumesh run`) | Host (Docker) | Host | Shipped |
| **Desktop** (`gpumesh desktop`) | Host (full OS session) | Host | Shipped (RDP/VNC tunnel) |
| **Hybrid launcher** (`gpumesh app`) | Host (Docker / optional desktop) | Host | **Shipped (R1)** |
| **GPU remoting** (`gpumesh cuda`) | Client uses remoting API | Host ops | **Shipped spike (R2)** — not drop-in libcuda |

This document covers remoting research. **R2 spike** is shipped as [`gpumesh cuda`](./cuda-remote.md) (narrow Runtime-API subset + host-memory backend). Full drop-in `libcuda` remains unsolved.

---

## 2. Why this is hard

A local app talks to the GPU through OS + driver stacks, for example:

```text
App → (OpenGL / Vulkan / DirectX / CUDA / Metal) → GPU driver → physical GPU
```

To use a **remote** GPU while the app stays local, something must:

1. **Intercept or replace** that API boundary on the client  
2. **Ship commands + data** (shaders, buffers, textures, kernels, frames) over the network  
3. **Execute** on the host GPU  
4. **Return results** (often frames, sometimes buffers) with low enough latency

Constraints that make “any app” extremely hard:

| Challenge | Why it matters |
| --- | --- |
| **API surface** | CUDA alone is huge; D3D12/Vulkan even larger; apps mix APIs |
| **Latency** | Interactive GUIs need ~16–50 ms round trips; WAN often can’t |
| **Bandwidth** | Textures, framebuffers, model weights move constantly |
| **State sync** | GPU context is sticky; remoting must track enormous state |
| **Driver coupling** | Apps assume a local NVIDIA/AMD stack and specific versions |
| **Security** | Remoting the GPU is almost as privileged as remoting the OS |
| **Anti-cheat / DRM** | Many apps detect modified graphics stacks |
| **Platform differences** | Win client → Linux host (or reverse) breaks many assumptions |

**Honest conclusion:** full “any local GUI app → remote GPU” is a **multi-year systems project**, closer to cloud gaming / GPU virtualization vendors than to a P2P job CLI.

---

## 3. Solution families (taxonomy)

### A. API remoting (true local app, remote GPU)

Client intercepts graphics/compute APIs; host runs a GPU server.

| Variant | Examples / prior art | Fits |
| --- | --- | --- |
| **CUDA remoting** | rCUDA, NVIDIA CUDA MPS + custom proxies, research systems | ML/training tools that are CUDA-only |
| **OpenGL remoting** | VirtualGL (usually “render remote, display local”), Chromium Remote, older SGI GLX remoting | Some OpenGL apps |
| **Vulkan/DX remoting** | Research / proprietary cloud workstation stacks | Hardest for consumer apps |
| **Full graphics stack virtualization** | Bits of Parsec/CloudXR/GeForce NOW architectures (mostly not open) | Closest to “any game/app” |

**Pros:** Matches the user mental model (“my app, their GPU”).  
**Cons:** Per-API; brittle; high latency sensitivity; incomplete coverage.

### B. Frame remoting (app runs on host; you only see pixels)

This is **not** local-app remoting — it’s what we already approximate with **RDP/VNC/Sunshine**.

**Pros:** Any app works if it runs on the host.  
**Cons:** App is not local; must install/run software on host.

### C. Hybrid “appear local, run remote”

Client UI is local; workload is packaged and executed on host automatically.

| Pattern | Behavior |
| --- | --- |
| **Transparent job launch** | Double-click / CLI wrapper syncs files, `run` on peer, streams logs/previews |
| **Local thin front-end** | Local Blender “controller” that submits render jobs remotely (add-on) |
| **Network filesystem + remote process** | App binary still on host; data via remount |

**Pros:** Achievable on GPUMesh foundation quickly.  
**Cons:** Not true API remoting; some UX friction.

### D. Device-level / SR-IOV / vGPU

Partition a physical GPU and attach a virtual GPU to a VM; client is a remote desktop into that VM.

**Pros:** Strong isolation; vendor-supported in data centers.  
**Cons:** Needs datacenter GPUs / licenses (vGPU); not realistic for two laptops on Wi‑Fi.

---

## 4. Prior art (what exists in the wild)

### Compute-focused

| Project / product | Notes |
| --- | --- |
| **rCUDA** | Academic CUDA API remoting over InfiniBand/Ethernet; cluster-oriented |
| **CUDA + RPC research** | Many papers; rarely production-ready for arbitrary apps |
| **Horovod / NCCL / RPC frameworks** | Distributed *training*, not local GUI apps |

### Display / interactive

| Project / product | Notes |
| --- | --- |
| **VirtualGL** | Render on remote GLX; composite locally — still assumes remote X/app placement often |
| **Sunshine + Moonlight** | Host encodes frames; client decodes — app on host |
| **Parsec / Steam Remote Play / GeForce NOW** | Polished frame remoting; not “local process + remote CUDA” |
| **Microsoft RemoteFX / RDP GFX** | Desktop remoting with GPU assist |
| **NVIDIA CloudXR** | XR streaming; proprietary |

### Takeaway for GPUMesh

Open, general, “any Windows app keeps its `.exe` local and uses peer RTX over the internet” **does not have a simple open-source drop-in**. Successful products almost always move the **application process** next to the GPU (frame remoting), or narrow to **one API** (CUDA).

---

## 5. Feasibility by app class

| App class | Feasibility of true local→remote GPU | Better approach on GPUMesh |
| --- | --- | --- |
| Python/PyTorch/TF training scripts | Medium (CUDA remoting or just `run`) | Keep **`gpumesh run`** |
| Blender CLI render | High via jobs | `run` + Blender image |
| Blender GUI | Low for API remoting | **Desktop tunnel** (already) or Blender remote render add-on |
| Unity / Unreal editor | Very low for API remoting | Desktop tunnel |
| Premiere / DaVinci | Very low | Desktop tunnel |
| Games | Extremely low (anti-cheat) | Frame remoting products, not P2P hobby stacks |
| Custom CUDA CLI tools | Medium | CUDA remoting spike or `run` |
| WebGPU / browser | Research | Not near-term |

---

## 6. Architecture options for GPUMesh (if we build remoting)

### Option 1 — CUDA remoting spike (narrow, honest)

```text
Client app (linked with stub libcuda)
        │  remoted calls + memory ops
        ▼
gpumesh-remoted  (QUIC, authenticated)
        ▼
Host agent → real libcuda → NVIDIA GPU
```

**Scope:** Linux/Windows CUDA apps that can use a replacement `libcuda` / ICD.  
**Out of scope:** D3D, Vulkan GUI editors, games.

**Build pieces:**

1. Interposition library (or CUDA forward-compatible stub)  
2. Wire protocol for subset of CUDA Driver/Runtime API  
3. Host-side executor process with real driver  
4. Memory: pin, migrate HtoD/DtoH over QUIC  
5. Auth: reuse pairing + new capability `gpu_remote` (stricter than desktop)  
6. Latency metrics + “LAN only” warning in doctor  

**Success metric:** `nvidia-smi`-class and a small CUDA sample + one PyTorch training script over remoting on LAN.

### Option 2 — Graphics remoting (OpenGL first)

```text
Client GL app → remoted libGL/libEGL
        ▼
Host GL server → GPU → optional frame return
```

Usually worse UX than desktop streaming unless LAN + careful design. Prefer Option 3 for GUI.

### Option 3 — Product-honest hybrid (recommended near-term)

Keep local UX, run process on host:

```text
Client: gpumesh app-run blender
  1) sync project (cp / watch)
  2) ensure Blender on host (or container with display)
  3) either:
       a) attach desktop session already open, or
       b) start headless/job and stream preview frames
```

**Feels like:** “I launched my project against their GPU.”  
**Actually is:** orchestrated remote execution + optional desktop.

**Shipped as** `gpumesh app sync` / `run` / `pull`.

### Option 4 — Vendor stack integration

Shell out / integrate Moonlight/Sunshine or CloudXR; GPUMesh = identity, allowlist, discovery, billing later.

Already partially the idea behind `desktop` + RDP. Extending to Sunshine UDP is incremental; it still won’t make local `.exe` use remote CUDA.

---

## 7. Protocol / security sketch (for Option 1)

New capability (do **not** overload job allow or desktop allow):

```text
allowlist.gpu_remote_allowed ⊆ paired peers
```

Session:

1. Signed Hello (existing)  
2. `GpuRemoteOpen { api: "cuda", client_ver }`  
3. Host checks capability + VRAM/util gates  
4. Stream of `Call` / `MemCopy` / `Event` messages  
5. `GpuRemoteClose`

Threats:

| Threat | Mitigation |
| --- | --- |
| Peer runs hostile kernels | Same as jobs: trust model; later sandbox / time limits |
| Memory DoS | Cap allocations; kill session |
| Side channels | Accept residual risk on shared GPU |
| Spoofed API client | Signed sessions; pin peer key |

Control plane: metadata only — **never** proxy CUDA traffic.

---

## 8. Network reality check

| Link | Interactive CUDA remoting | Frame remoting (desktop) | Batch jobs |
| --- | --- | --- | --- |
| Same LAN, &lt;2 ms | Maybe usable for compute tools | Good | Excellent |
| Wi‑Fi / ~10–30 ms | Painful for GUI; OK for some training | OK–good with encoders | Excellent |
| Internet / 50–100 ms+ | Poor for “local app feel” | Needs heavy encode (Parsec-class) | Excellent |

**Implication:** Market “local apps on their GPU” for **WAN** without saying it’s batch/jobs or desktop streaming will disappoint users.

---

## 9. Recommended roadmap for GPUMesh

### R0 — Document & set expectations *(this file)*

- Clarify three modes: jobs / desktop / remoting  
- Do not advertise remoting as shipped  

### R1 — Hybrid launcher (highest ROI) — **done**

```bash
gpumesh app sync --peer alice-pc --dir ./proj
gpumesh app run --peer alice-pc --dir ./train python train.py
gpumesh app run --peer alice-pc --dir ./blend --out ./renders --desktop blender -b scene.blend -a
gpumesh app pull --peer alice-pc --job <id> --dir ./out
```

- Packages the local directory (`.gpumeshignore`), runs the command **on the peer** next to the GPU  
- `--out` / `app pull` fetches `outputs.gpk` from the job workspace  
- `--desktop` prints `gpumesh desktop connect <peer>` (GUI still runs on the host)  
- Honest banner: this is **not** a local `.exe` using a remote CUDA device  

### R2 — CUDA remoting spike (LAN only) — **done (narrow)**

```bash
gpumesh cuda share
gpumesh cuda allow bob-laptop
gpumesh cuda demo --peer alice-pc
gpumesh cuda bench --peer alice-pc --iters 50
```

- Capability `gpu_remote_allowed` (does **not** grant jobs)  
- Ops: device query, malloc/free, memcpy, memset, sync, built-in `vector_add_f32`  
- Backend: `host-memory` + NVML identity (not full driver kernel launch)  
- Docs: [cuda-remote.md](./cuda-remote.md)  
- **Not included:** PyTorch / arbitrary CUDA apps / ICD interposition  

### R3 — Expand remoting only if R2 metrics look good

- Real CUDA driver / PTX kernel path  
- Broader Runtime API  
- Optional OpenGL path  
- Never promise D3D12/Vulkan “all Steam games” without a dedicated graphics team  

### Explicit non-goals (near term)

- GeForce NOW clone  
- Anti-cheat games  
- macOS Metal remoting  
- Transparent replacement of every system GPU DLL for all ISV software  

---

## 10. Decision matrix

| Goal | Build |
| --- | --- |
| Train / scripts on peer GPU | **Already: `gpumesh run`** |
| Use Blender/Unity GUI on peer GPU | **Already: `gpumesh desktop`** (app on host) |
| Keep `.exe` local, GPU remote, any app | **Research; not R2** — spike is remoting API, not ICD |
| Keep project local-feeling, GPU remote | **Shipped: `gpumesh app` (R1)** |
| CUDA-only tools, LAN | **Shipped spike: `gpumesh cuda` (R2)** — expand in R3 |

---

## 11. Open research questions

1. Which CUDA API subset covers 80% of ML CLI tools?  
2. Can we interpose on Windows without signing/driver issues?  
3. How to version-match client stub vs host driver?  
4. Multiplexing multiple client apps on one host GPU safely?  
5. Should remoting sessions share the same Docker isolation story as jobs?  
6. Is WebGPU remoting a cleaner long-term bet than CUDA DLL injection?  

---

## 12. Summary

| Question | Answer |
| --- | --- |
| Can GPUMesh do local-app → remote-GPU today? | **Partially** — `gpumesh cuda` remotes a small API; **not** drop-in libcuda |
| Can users still use peer GPUs for apps today? | **Yes** — **`gpumesh app`**, **desktop**, **jobs**, or **`cuda` demo** |
| Can we eventually do true remoting? | **Maybe** — R3 if bench/demo metrics on LAN look usable |
| Best next product step | **R3** only if R2 p50 RTT is acceptable on your LAN; else stay on app/desktop |

---

## 13. References / further reading (starting points)

- rCUDA project papers and docs  
- VirtualGL architecture  
- NVIDIA documentation on MPS, vGPU, CloudXR (capability boundaries)  
- Cloud gaming encoder pipelines (why frame remoting won for “any app”)  
- GPUMesh [PRD](../PRD.md) phases 8–12 (reputation, credits, marketplace, inference) — orthogonal to remoting but relevant if GPU time is monetized  

---

*Last updated: R2 CUDA remoting spike shipped (`gpumesh cuda`). Full libcuda interposition still research / R3.*
