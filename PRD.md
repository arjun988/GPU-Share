# PRD — GPUMesh

**Peer-to-Peer GPU Sharing & Compute Network**

| Field | Value |
| --- | --- |
| **Product** | GPUMesh |
| **Tagline** | Turn idle GPUs into a personal compute network. |
| **Type** | Open-source, CLI-first P2P GPU compute platform |
| **Primary users** | AI/ML developers, students, researchers, GPU owners, homelab users, small teams |
| **License** | Apache-2.0 (recommended; align repo `LICENSE` before public release) |
| **Initial platform** | Linux + NVIDIA GPUs (Windows support immediately after Linux MVP) |
| **Document status** | Implementation baseline — treat as source of truth for Phases 0–3 |
| **Version** | 1.0 |

---

## 0. How to use this document

This PRD is written so engineering can execute with minimal reinterpretation.

- **Phases 0–3** are the build contract. Do not expand scope into later phases until Phase 3 acceptance criteria pass.
- **CLI contracts** in §7–§13 are normative for MVP. Command names and flags listed there should ship as specified unless a decision log entry changes them.
- **Non-goals (§37)** are hard exclusions for MVP and early phases.
- **Decisions (§42)** are frozen unless explicitly revisited.
- Later phases (4–12) are directional roadmap, not current build requirements.

**North-star MVP proof**

```text
Provider:  gpumesh init && gpumesh share
Consumer:  gpumesh pair <code> && gpumesh run --peer <name> python train.py
```

Outcome: authenticated, encrypted P2P job on the provider GPU inside an isolated container, with streamed logs and file transfer — no SSH, VPN, or manual port forwarding.

---

## 1. Product vision

GPUMesh lets people share idle GPU compute with other machines over a secure peer-to-peer network.

**Provider**

```bash
gpumesh share
```

**Consumer**

```bash
gpumesh peers
gpumesh run --peer alice python train.py
```

The workload runs on the peer’s GPU; the consumer interacts remotely through the CLI.

**Long-term vision**

> A permissionless, open-source P2P compute network where GPU resources can be shared and consumed like local resources.

**Strategic positioning (locked)**

Do **not** launch as a GPU marketplace.

Launch as:

> **GPUMesh — P2P GPU sharing for developers.**  
> Share your idle GPU. Use someone else’s GPU. No cloud required.

Growth path:

```text
friends → teams → private clusters → public network → marketplace → inference network
```

---

## 2. Problem

GPU compute is expensive and fragmented. A developer may have an idle gaming GPU, a CPU-only laptop, a friend’s workstation, lab machines, or overnight-idle office GPUs — but stitching them together requires SSH, VPN, port forwarding, Docker, CUDA setup, auth, networking, file transfer, and process management.

GPUMesh abstracts that into:

```bash
gpumesh connect <peer>
gpumesh run python train.py
```

---

## 3. Core concept

Three layers:

```text
                    GPUMesh Network

              ┌─────────────────────┐
              │   Control Plane     │
              │ Discovery / Identity│
              │ Signaling / Metadata│
              └──────────┬──────────┘
                         │ discovery only
            ┌────────────┴────────────┐
            │                         │
       GPU Consumer              GPU Provider
       GPUMesh CLI               GPUMesh Agent
            │                         │
            └───────────┬─────────────┘
                        │ P2P QUIC
                ┌───────▼───────┐
                │ Secure Runtime│
                └───────┬───────┘
                        │ Container
                        ▼
                   NVIDIA GPU
```

**Hard rule:** the control plane must **not** execute workloads. Workload bytes should flow consumer ↔ provider (direct P2P preferred; relay only as fallback).

---

## 4. Product principles

| # | Principle | Requirement |
| --- | --- | --- |
| 4.1 | CLI first | CLI is the primary product; dashboard is secondary and post-MVP |
| 4.2 | P2P by default | Do not route compute through centralized servers |
| 4.3 | Secure by default | Remote users never get unrestricted host access |
| 4.4 | Containerized workloads | Remote jobs run in isolated environments |
| 4.5 | Open protocols | Protocol must be documentable for third-party clients |
| 4.6 | Local-first | LAN operation must work if the central service is unavailable |
| 4.7 | No blockchain | Blockchain is out of scope for the core product |

---

## 5. Target users

| Persona | Need |
| --- | --- |
| **GPU owner** | Share idle RTX/A-series GPUs with friends, teammates, researchers |
| **AI developer** | Run training/inference without buying cloud GPUs |
| **Student** | Form a private “university GPU cluster” from friends’ machines |
| **Homelab user** | Unify PC/server/NAS/workstation into one compute network |
| **Research team** | Share internal workstation GPUs with auth and limits |

---

## 6. MVP scope

**MVP goal:** make GPU sharing between **trusted peers** ridiculously easy.

**Explicitly not MVP:** public marketplace, payments, credits, reputation, multi-GPU distributed training, dashboard, public discovery.

### 6.1 MVP must-have features

| Area | Features |
| --- | --- |
| GPU | Detection, monitoring (VRAM, util, temp, driver/CUDA) |
| Identity | Local node identity (Ed25519), pairing |
| AuthZ | Explicit allow/deny of peers |
| Network | P2P connect, NAT traversal, relay fallback |
| Runtime | Container execution, NVIDIA GPU passthrough |
| Jobs | Package, transfer, run, stream logs, lifecycle, cancel, exit codes |
| Limits | VRAM/CPU/RAM/disk/runtime/concurrency caps |
| Ops | Status, logs, graceful share stop / job termination |

### 6.2 MVP success criteria (Definition of Done)

All of the following must pass on real hardware (two Linux NVIDIA machines, preferably across NAT):

1. Provider: `gpumesh init` → `gpumesh share`
2. Consumer: `gpumesh pair <code>` → peer appears in `gpumesh peers`
3. `gpumesh run --peer <provider> --image <cuda/pytorch image> <command>` starts a container with GPU access
4. Required files transfer; stdout/stderr stream live; exit code returns correctly
5. Connection is authenticated and encrypted; denied peers cannot run jobs
6. No manual SSH, VPN, or port-forward setup required for the happy path
7. Provider can stop sharing and running jobs terminate gracefully
8. Resource limits are enforced (at least max VRAM and max concurrent jobs)

---

## 7. CLI contract (normative for Phases 1–3)

Binary name: `gpumesh`  
Provider daemon may be the same binary in agent mode or `gpumesh-agent` (implementation choice; UX must feel like one product).

### 7.1 Identity & status

```bash
gpumesh init                 # create local identity + config
gpumesh status               # node, GPU, network, peer count
gpumesh gpu                  # detailed GPU inventory
gpumesh doctor               # Phase 4 preferred; optional early for diagnostics
```

**`gpumesh status` example output**

```text
GPUMesh Status

Node: arjun-desktop
Node ID: 12ab34...

GPU:
  NVIDIA RTX 4090
  VRAM: 24 GB
  Utilization: 7%
  Temperature: 43°C

Network:
  P2P: Connected
  Peers: 2
```

### 7.2 Auth (post-MVP control plane)

```bash
gpumesh login                # reserved for cloud/dashboard later
```

Not required for Phase 3 private P2P.

### 7.3 Sharing

```bash
gpumesh share
gpumesh share --max-vram 16GB --max-gpu-utilization 80
gpumesh share stop
```

**`gpumesh share` example output**

```text
GPUMesh

GPU: RTX 4090
VRAM: 24 GB
Available: 20 GB

Sharing enabled.
Waiting for authorized peers...
```

### 7.4 Pairing & peers

```bash
gpumesh pair <code>
gpumesh peers
gpumesh allow <peer>
gpumesh deny <peer>
gpumesh connect <peer>       # ensure/refresh session to peer
```

**`gpumesh peers` example**

```text
NAME          GPU          VRAM     STATUS
alice-pc      RTX 4090     24GB     IDLE
bob-server    RTX 3090     24GB     BUSY
lab-a         A6000        48GB     IDLE
```

Later (not MVP): `gpumesh discover` for public nodes.

### 7.5 Remote execution

```bash
gpumesh run --peer <name> [--image <image>] [--env KEY=VAL] [--workdir <path>] <command...>
```

Canonical example:

```bash
gpumesh run --peer alice-pc --image pytorch/pytorch:latest python train.py
```

**Runtime pipeline (required)**

1. Package workload (respect ignore rules when present)
2. Transfer required files
3. Establish/reuse P2P connection
4. Create isolated container
5. Attach GPU per policy/limits
6. Start process
7. Stream logs (stdout/stderr)
8. Track resources
9. Return artifacts/exit code
10. Support cancel / graceful termination

### 7.6 Jobs

```bash
gpumesh jobs
gpumesh cancel <job-id>
gpumesh logs <job-id>
```

Job IDs must be stable, unique per node (e.g. short hex).

### 7.7 File transfer

```bash
gpumesh cp <local> <peer>:<remote>
gpumesh cp <peer>:<remote> <local>
```

Example:

```bash
gpumesh cp dataset.zip alice:/data/
gpumesh cp alice:/output/model.safetensors ./output/
```

MVP: correct transfer + progress. Later: resumable, chunked, compressed, deduplicated, content-addressed.

### 7.8 Interactive shell (Phase 3+, gated)

```bash
gpumesh exec <peer> bash
# or alias-style: gpumesh ssh <peer>
```

**Must** be an isolated workload shell inside the sandbox/container — **never** unrestricted host SSH.

### 7.9 Config & policy (Phase 4 preferred; stubs OK earlier)

```bash
gpumesh config
gpumesh policy               # later: richer ACL/policy UI
gpumesh update
```

---

## 8. Peer discovery

| Mode | When | Behavior |
| --- | --- | --- |
| Pairing | MVP | Out-of-band code exchanges peer IDs / trust |
| LAN | MVP | Local discovery without control plane if possible |
| Rendezvous | Phase 2+ | Optional control-plane signaling for WAN |
| Public discover | Phase 7 | Searchable public nodes |

Local-first: paired peers on the same LAN must work offline from the control plane.

---

## 9. Remote execution requirements

| Requirement | Spec |
| --- | --- |
| Isolation | Docker (MVP); no host shell |
| GPU | NVIDIA Container Toolkit passthrough |
| Image | `--image` supported; sensible default documented |
| I/O | Stream stdout/stderr; capture exit code |
| Lifecycle | create → running → succeeded/failed/cancelled |
| Limits | Enforce provider share policy |
| Monitoring | Report GPU util/VRAM during job when available |

**Live job UX example**

```text
Job: 8f21c

GPU: RTX 4090
VRAM: 11.2 / 24 GB
GPU utilization: 94%

Running...

Epoch 12/50
loss: 0.0421
```

---

## 10. Interactive shell

See §7.8. Non-negotiable: sandbox only, not host access.

---

## 11. File transfer

MVP requirements:

- Bidirectional `gpumesh cp`
- Progress indication
- Failures are clear and non-corrupting (no partial silently treated as success)

Post-MVP:

- Resumable transfers, chunking, compression, deduplication, content-addressed storage
- Dataset caching / peer-local datasets for large ML data

---

## 12. Container support

```bash
gpumesh run --image pytorch/pytorch:latest python train.py
gpumesh run --image nvidia/cuda:12.8.0-runtime ./app
```

Container must support:

```text
CPU limits | RAM limits | filesystem limits | network restrictions | GPU access
```

MVP runtime: **Docker**. Later options (containerd / youki / Kata) only if security needs demand it.

---

## 13. GPU resource controls

Provider controls (CLI + config file):

| Control | Examples |
| --- | --- |
| VRAM | `--max-vram 16GB` |
| GPU utilization | `--max-gpu-utilization 80` |
| Host resources | max CPU, RAM, disk |
| Jobs | max runtime, max concurrent jobs |
| Network | bandwidth caps (later OK) |
| Policy | allowed images, allowed users/peers |

Scheduler must account for **available** VRAM, not only total VRAM.

---

## 14. Security model

### 14.1 Threat model (MVP)

Never allow:

```text
Remote user → host shell → full machine access
```

Required path:

```text
Remote user → authenticated P2P → job sandbox → container → GPU
```

Assume hostile workloads may attempt: container escape, host FS access, network scanning, GPU abuse, resource exhaustion.

### 14.2 Identity

- Every node has a cryptographic identity
- Algorithm: **Ed25519** (peer ID + signatures)

### 14.3 Encryption

All peer communication encrypted.

Recommended stack:

```text
libp2p + QUIC (Quinn) with TLS 1.3
```

Noise is acceptable if a libp2p security choice requires it; document the chosen handshake.

### 14.4 Authorization

- Default deny for remote run/exec/cp
- Explicit `allow` / `deny`
- Pairing establishes candidate trust; owner still controls access policy

### 14.5 Secrets & data

- Private workload contents must not be stored on the control plane
- Local config/keys stored with restrictive filesystem permissions

---

## 15. Networking architecture

| Layer | Choice | Role |
| --- | --- | --- |
| Identity / discovery / NAT / connections | **libp2p** | Peer identity, discovery, NAT traversal, connection mgmt |
| Transport | **QUIC** | Low latency, multiplexed streams, reliability |
| Fallback | **TURN / relay** | When direct P2P fails |

```text
             Rendezvous Server
                    │
             discovery / signaling only
                    │
       ┌────────────┴────────────┐
    Consumer                 Provider
       │                         │
       └────── QUIC / P2P ───────┘

If direct fails:
Consumer → Relay → Provider
```

Relay is for connectivity, not preferred data path. Prefer direct; fall back transparently; surface connection mode in status/logs.

---

## 16. GPU support

| Phase | Scope |
| --- | --- |
| Phase 1 (MVP) | **NVIDIA only**: CUDA, NVML, NVIDIA Container Toolkit |
| Phase 2 GPU | AMD ROCm |
| Later | Intel / Apple Silicon / other accelerators |

**NVIDIA metrics to expose**

```text
model, VRAM total/used, temperature, power, utilization,
CUDA version, compute capability, driver version
```

---

## 17. Runtime architecture

Provider runs an agent (`gpumesh` agent mode or `gpumesh-agent`):

```text
gpumesh-agent
├── Identity Manager
├── Network Manager
├── Peer Manager
├── Authentication / Authorization
├── Job Manager
├── Container Manager
├── GPU Manager
├── Resource Monitor
├── File Manager
├── Security Manager
└── Logging
```

Consumer CLI talks to the remote agent over the P2P protocol. Local `gpumesh run` (no `--peer`) uses the same runtime path on the local agent (Phase 1 milestone).

---

## 18. Tech stack (locked for Phases 0–3)

### 18.1 Core (Rust)

**Language:** Rust  
**Async:** Tokio  
**CLI:** Clap  
**Serde / Tracing / Reqwest** as needed  
**Network:** libp2p + Quinn (QUIC)

Crates responsibility map:

| Crate | Responsibility |
| --- | --- |
| `gpumesh-cli` | User-facing CLI |
| `gpumesh-agent` | Provider daemon |
| `gpumesh-core` | Shared domain logic |
| `gpumesh-network` | libp2p/QUIC wiring |
| `gpumesh-protocol` | Messages, versioning, framing |
| `gpumesh-runtime` | Jobs + container orchestration |
| `gpumesh-gpu` | NVML/CUDA wrappers |
| `gpumesh-security` | Identity, authZ, crypto helpers |
| `gpumesh-storage` | Local state, file packaging/transfer |
| `gpumesh-common` | Errors, config, shared types |

### 18.2 GPU / containers

- CUDA + NVIDIA NVML + NVIDIA Container Toolkit
- Docker for MVP

### 18.3 Control plane (Phase 2+; minimal for rendezvous)

- **Go** + gRPC or Connect
- **PostgreSQL** when persistence needed
- Responsibilities: auth (later), rendezvous, signaling, node metadata, dashboard API
- **Never** runs GPU workloads

### 18.4 Dashboard (Phase 6 only)

- Next.js, TypeScript, Tailwind CSS, shadcn/ui

### 18.5 Infra

- Docker, GitHub Actions
- Observability later: OpenTelemetry, Prometheus, Grafana

---

## 19. Dashboard (Phase 6)

Build only after CLI + private P2P + remote execution are solid.

Pages: Overview, My GPUs, Peers, Jobs, Network, Usage, Settings, Security.

Not part of MVP acceptance.

---

## 20. Data storage

| Phase | Storage |
| --- | --- |
| MVP | **No central DB** — local files/keys/state only |
| Phase 2+ control plane | PostgreSQL for users, nodes, peer relationships, jobs metadata, GPU metadata, usage, API tokens, network stats |

Never store private workload payloads centrally unless the user explicitly opts into a feature that requires it.

---

## 21. Observability

| Phase | Approach |
| --- | --- |
| Early | Rust `tracing`, structured logs |
| Later | OpenTelemetry + Prometheus + Grafana |

**Metrics to plan for**

```text
GPU util, VRAM util, job duration, network throughput/latency,
job failures, peer uptime, container failures
```

---

## 22. Repository structure

Monorepo:

```text
gpumesh/
├── crates/
│   ├── gpumesh-cli/
│   ├── gpumesh-agent/
│   ├── gpumesh-core/
│   ├── gpumesh-network/
│   ├── gpumesh-protocol/
│   ├── gpumesh-runtime/
│   ├── gpumesh-gpu/
│   ├── gpumesh-security/
│   ├── gpumesh-storage/
│   └── gpumesh-common/
├── services/
│   └── control-plane/
├── dashboard/
├── docs/
├── examples/
├── tests/
├── scripts/
├── Cargo.toml
├── README.md
├── LICENSE
├── CONTRIBUTING.md
└── PRD.md
```

Protocol and runtime stay in-repo together.

---

## 23. Phase 0 — Research & architecture

**Goal:** validate hardest assumptions with tiny prototypes.

**Prototypes required**

- GPU detection (NVML)
- Docker GPU execution
- QUIC connection
- libp2p connection
- NAT traversal (+ relay fallback spike)
- File transfer
- Remote log streaming

**Deliverable**

```text
Machine A ──P2P──► Machine B ──► Docker ──► RTX GPU
```

No dashboard, marketplace, or accounts.

**Exit criteria:** written spike notes in `docs/` + working demo script between two machines.

---

## 24. Phase 1 — Local GPU agent

**Commands**

```bash
gpumesh status
gpumesh gpu
gpumesh share
gpumesh share stop
gpumesh run <command...>   # local runtime path
```

**Features**

- GPU detection & monitoring
- Agent daemon
- Resource limits
- Local job execution via Docker + GPU

**Exit criteria:** `gpumesh run python train.py` works locally through GPUMesh runtime (containerized, monitored).

---

## 25. Phase 2 — Private P2P network

**Commands**

```bash
gpumesh init
gpumesh pair
gpumesh peers
gpumesh connect
gpumesh allow / gpumesh deny
```

**Features**

- Ed25519 identity
- Peer auth + encrypted connections
- P2P networking, NAT traversal, relay fallback
- Peer authorization

**Exit criteria:** two friends securely connect machines across typical home NATs without manual port forwarding.

---

## 26. Phase 3 — Remote GPU execution (first product milestone)

**Example**

```bash
gpumesh run \
  --peer alice \
  --image pytorch/pytorch:latest \
  python train.py
```

**Must implement**

- Workload packaging
- Container creation + GPU passthrough
- File transfer
- stdout/stderr streaming
- Exit codes, job IDs, cancellation
- Resource monitoring during run

**Exit criteria:** §6.2 MVP success criteria fully met.

---

## 27. Phase 4 — Developer experience

- Installer: `curl -fsSL install.gpumesh.dev | sh` (+ platform packages)
- `gpumesh doctor`, `update`, `logs`, `config`, `jobs`, `cancel`
- Shell autocomplete
- Config files, YAML job defs, env vars
- `.gpumeshignore`
- Job retries, resumable transfers

---

## 28. Phase 5 — Private GPU clusters

```bash
gpumesh group create research
gpumesh run --gpu-memory 20GB train.py
```

Group membership + scheduler selects a suitable idle peer. Lightweight P2P GPU cluster manager.

---

## 29. Phase 6 — Dashboard

GPUMesh Cloud UI for overview, GPUs, jobs, peers, settings. Secondary to CLI.

---

## 30. Phase 7 — Public GPU network

```bash
gpumesh share --public
gpumesh search --gpu 4090
```

Publish metadata only (GPU, VRAM, CUDA, availability, perf, latency, region, uptime). Still no marketplace requirement.

---

## 31. Phase 8 — Reputation

Provider/consumer scores from completions, failures, uptime, latency, abuse reports. Enables safer public sharing.

---

## 32. Phase 9 — GPU credits

Optional centralized credits (not blockchain). Owners earn; consumers spend. Explore decentralized settlement only if clearly needed later.

---

## 33. Phase 10 — Marketplace

Price-aware scheduling (`--max-price`, reliability, VRAM). Separate product surface from core sharing.

---

## 34. Phase 11 — Multi-GPU

```bash
gpumesh run --gpus 4 train.py
```

**Late-stage only.** Cross-internet distributed training has poor latency/bandwidth characteristics. Early product prioritizes **single-GPU** workloads.

---

## 35. Phase 12 — AI inference network

```bash
gpumesh serve llama
gpumesh inference <model>
```

Providers run inference servers; scheduler routes clients. Distinct from generic `run`.

---

## 36. Future AI integrations

Target ecosystems (post-MVP): PyTorch, TensorFlow, JAX, CUDA, vLLM, Ollama, llama.cpp, Hugging Face, Docker, Jupyter.

Convenience flags later:

```bash
gpumesh run --framework pytorch
gpumesh serve --model llama
gpumesh notebook
```

---

## 37. Non-goals (MVP / early phases)

Do **not** build initially:

| Excluded | Reason |
| --- | --- |
| Blockchain / cryptocurrency | Unnecessary for core sharing |
| Kubernetes replacement | Wrong abstraction for MVP |
| Decentralized storage network | Out of scope |
| Custom GPU driver / CUDA runtime | Use NVIDIA stack |
| Public marketplace / payments | Premature |
| Distributed multi-node training | Latency/bandwidth reality |
| Dashboard / mobile app | After CLI product works |

MVP proves one question only:

> Can I safely and easily use someone else’s idle GPU as if it were my own?

---

## 38. Technical risks & mitigations

| Risk | Mitigation |
| --- | --- |
| **NAT / CGNAT / corporate firewall** | Direct P2P → hole punching → relay fallback; test early in Phase 0/2 |
| **Security / container escape** | Default deny, isolation, image allowlists, resource caps, least privilege, security review before public sharing |
| **Large dataset transfer** | Progress + later caching/chunking/dedup/peer-local datasets; document that transfer can dominate runtime |
| **VRAM fragmentation** | Schedule on **available** VRAM; reject/queue when insufficient |
| **Unreliable peers** | Heartbeats, timeouts, cancel, clear failure modes; later: retry, checkpoint, reschedule |

---

## 39. Product metrics

### MVP

```text
Successful P2P connections
Connection latency
Job startup time
Job failure rate
File transfer speed
GPU utilization during jobs
```

### Network (later)

```text
Active nodes, online GPUs, total VRAM,
GPU-hours contributed/consumed, jobs/day, average uptime
```

### Marketplace (eventually)

```text
Supply/demand, average price, provider earnings, consumer spending
```

---

## 40. Definition of success

MVP succeeds when this workflow is reliable end-to-end:

**Provider**

```bash
gpumesh init
gpumesh share
```

**Consumer**

```bash
gpumesh pair <provider-code>
gpumesh run --peer provider python train.py
```

**Path**

```text
Consumer laptop ──Internet/P2P──► Provider PC ──► Docker ──► RTX GPU ──► training.py
```

With: secure auth, encrypted P2P, no manual port forward/SSH/VPN, isolated workload, streamed logs, file transfer, GPU monitoring.

If that experience is excellent, the rest of GPUMesh can grow around it.

---

## 41. Final recommended stack (summary)

```text
CLIENT:   Rust CLI
PROVIDER: Rust Agent
NETWORK:  libp2p + QUIC (encrypted P2P)
RUNTIME:  Docker + NVIDIA Container Toolkit + CUDA/NVML
CONTROL:  Go + gRPC/Connect + PostgreSQL (signaling/metadata only)
DASHBOARD (later): Next.js + TypeScript + Tailwind + shadcn/ui
CI/OBS:   GitHub Actions; later Prometheus/OTel
```

---

## 42. Frozen decisions (v1.0)

| ID | Decision |
| --- | --- |
| D1 | Product launches as private/trusted P2P sharing, not a marketplace |
| D2 | CLI is the primary interface through Phase 5 |
| D3 | Workloads never execute on the control plane |
| D4 | MVP OS/GPU: Linux + NVIDIA only |
| D5 | Implementation language for CLI/agent/protocol: Rust |
| D6 | Networking: libp2p + QUIC; relay fallback required |
| D7 | Identity: Ed25519 |
| D8 | Isolation: Docker containers; no host shell for remote users |
| D9 | MVP storage: local-only; no required central DB |
| D10 | No blockchain in core product |
| D11 | Single-GPU jobs are the early product; multi-GPU distributed training is late |
| D12 | License target: Apache-2.0 (update repo license before public launch if still MIT) |
| D13 | Phases 0–3 are sequential gates; do not start Phase 4+ features as blockers for Phase 3 |

---

## 43. Open items (non-blocking for Phase 0 start)

Resolve before or during Phase 2/3; defaults suggested:

| Item | Default if undecided |
| --- | --- |
| Single binary vs separate `gpumesh-agent` | Single binary with `gpumesh agent` / service subcommand |
| Default container image | Pin a documented CUDA runtime image in docs |
| Config path | `~/.gpumesh/` on Linux |
| Protocol versioning | Semver in handshake; reject incompatible majors |
| Relay hosting | Self-host optional relay in `services/`; document public relay later |
| Windows timeline | Start after Linux Phase 3 DoD, unless contributors land earlier |

---

## 44. Acceptance checklist — Phase 3 (ship gate)

- [ ] `gpumesh init` creates identity under the documented config path
- [ ] `gpumesh share` advertises GPU and accepts only allowed peers
- [ ] Pairing works with a short code / QR-alternative string
- [ ] `gpumesh peers` lists name, GPU, VRAM, status
- [ ] `gpumesh run --peer ...` executes in Docker with GPU
- [ ] Logs stream; exit code propagates; `cancel` works
- [ ] `gpumesh cp` round-trips files both directions
- [ ] Direct P2P preferred; relay used automatically when needed
- [ ] Denied peer cannot start jobs
- [ ] Share stop / agent shutdown cleans up containers
- [ ] Basic resource limits enforced
- [ ] README documents install, GPU prerequisites, and two-machine demo
- [ ] No marketplace/dashboard/payments code required to complete this checklist

---

*End of PRD v1.0 — execute Phases 0→3 against this document; revise only via explicit decision-log updates.*
