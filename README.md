# GPUMesh

**Turn idle GPUs into a personal compute network.**

GPUMesh is an open-source, CLI-first peer-to-peer platform for sharing NVIDIA GPUs across laptops and workstations you trust. Pair explicitly, run containerized jobs on a peer’s GPU, manage private clusters, and operate everything from a local dashboard — without sending workloads through a cloud control plane.

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-alpha-yellow.svg)](#status)

---

## Why GPUMesh?

| Problem | What GPUMesh does |
| --- | --- |
| Idle gaming / lab GPUs | Share them with teammates on the same LAN (or via relay) |
| Cloud GPU cost for small jobs | Run `nvidia-smi`, training scripts, and Docker workloads on a friend’s machine |
| “Just give them SSH” | Default-deny allowlist, Ed25519 identity, sandboxed Docker — not a raw shell |
| Fragmented tooling | One CLI + local ops dashboard for pair, run, logs, metrics, and connect |

**Not a GeForce NOW clone.** Interactive desktop and CUDA remoting exist as narrow, honest features — see [docs](#documentation). Core path: authenticated P2P → job sandbox → Docker → NVIDIA GPU.

---

## Features

- **Mutual pairing** — Ed25519 node identity; pairing codes carry trust + address hints
- **Docker NVIDIA jobs** — `gpumesh run` / `gpumesh app` on an allowed peer
- **Private clusters** — groups + idle/VRAM-aware scheduling (`run --group`)
- **Local dashboard** — live GPUs, peers, jobs, logs, pair/connect/run (light & dark UI)
- **Optional public registry** — metadata-only discovery; pair before any workload
- **Relay assist** — when direct QUIC fails across NAT
- **GPU desktop & CUDA remoting** — optional paths for GUI / Runtime-API experiments

---

## Status

GPUMesh is **alpha** (v0.1). Phases **0–7** of the product roadmap are implemented for day-to-day use: pairing, sharing, jobs, DX, groups, dashboard, and public registry. APIs and CLI flags may still change. Production marketplace / credits / multi-GPU training are **not** in scope yet — see [PRD.md](./PRD.md).

---

## Requirements

| Role | Needs |
| --- | --- |
| **Any node** | Rust toolchain (to build), `gpumesh` on `PATH` |
| **Provider (shares GPU)** | NVIDIA GPU, recent driver, [Docker](https://docs.docker.com/get-docker/) + [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html) |
| **Consumer** | Network reachability to the provider (same LAN is simplest) |
| **Dashboard (optional)** | Node.js 18+ for the Next.js UI |

Supported focus: **Linux + NVIDIA**. Windows / WSL work for many flows; macOS cannot share NVIDIA GPUs.

---

## Install

### From this repository

```bash
git clone https://github.com/gpumesh/gpumesh.git
cd gpumesh

# Installer (builds with cargo when run from a checkout)
GPUMESH_FROM_SOURCE=1 ./scripts/install.sh

# Or install the CLI directly
cargo install --path crates/gpumesh-cli
```

Ensure `~/.local/bin` (or your Cargo bin dir) is on `PATH`, then verify:

```bash
gpumesh --help
gpumesh doctor
```

---

## Quick start

### 1. Single machine

```bash
gpumesh start                 # interactive menu
# or step-by-step:
gpumesh init --name alice-pc
gpumesh doctor
gpumesh share                 # leave running to accept jobs
```

State lives under `~/.gpumesh` (identity, peers, jobs, logs).

### 2. Two machines (provider + consumer)

Pairing is **mutual**. Machines must reach each other (same Wi‑Fi / LAN is the easy path).

**Laptop A — provider (has the GPU)**

```bash
gpumesh init --name alice-pc
gpumesh doctor
gpumesh share                 # leave running; copy the pairing code it prints
```

**Laptop B — consumer**

```bash
gpumesh init --name bob-laptop
gpumesh pair '<alice-pairing-code>'
gpumesh peers
```

**Mutual allow** — A must also pair B. On B: `gpumesh pair-code`, then on A:

```bash
gpumesh pair '<bob-pairing-code>'
```

**Run a job from B on A’s GPU**

```bash
gpumesh run --peer alice-pc --image python:3.12-slim nvidia-smi
gpumesh jobs
gpumesh logs <job-id>
```

**Across NAT / different networks** — run a relay on a reachable host and point agents at it:

```bash
# Public host (UDP 4799 by default)
cargo run -p gpumesh-relay

# On each GPUMesh machine
export GPUMESH_RELAY=host:4799
```

Full walkthrough: [docs/two-machine-demo.md](./docs/two-machine-demo.md).

### 3. Private cluster (N machines)

```bash
# Owner
gpumesh group create research
gpumesh group invite research

# Members
gpumesh group join '<invite-code>'
gpumesh pair '<peer-pairing-code>'   # with providers you will use

# Providers
gpumesh share

# Anyone in the group
gpumesh run --group research --gpu-memory 8GB --image python:3.12-slim python train.py
```

### 4. Local dashboard

Live metrics, logs, pairing, connect, and run — no `gpumesh sync` required.

```bash
# Terminal A — API (reads ~/.gpumesh)
cargo run -p gpumesh-control

# Terminal B — UI
cd dashboard && npm install && npm run dev
```

Open **http://127.0.0.1:3000**. Or run `gpumesh dashboard` for the URLs.

---

## How it works

```text
  Consumer                         Provider
 ┌──────────┐   Ed25519 + QUIC    ┌──────────────────┐
 │ gpumesh  │ ─────────────────► │ gpumesh share     │
 │ run/app  │   allowlisted only  │   ↓ Docker        │
 └──────────┘                     │   ↓ NVIDIA GPU   │
                                  └──────────────────┘

  Control plane / dashboard = metadata + local ops API
  Workload bytes never transit the control plane
```

| Command | Role |
| --- | --- |
| `init` | Create node identity under `~/.gpumesh` |
| `pair` | Establish trust + allowlist between two nodes |
| `share` | Provider listens and accepts jobs from allowed peers |
| `run --peer` | Pin a job to one machine |
| `run --group` | Schedule onto an idle group member with enough VRAM |
| `deny` | Revoke access |

Default listen port: **UDP 47000** (configurable).

---

## CLI map

| Area | Commands |
| --- | --- |
| Core | `start` `init` `status` `gpu` `doctor` `share` `pair-code` `pair` `peers` `run` `search` |
| Apps | `app sync\|run\|pull` · `desktop …` · `cuda share\|allow\|demo\|bench\|doctor` |
| Clusters | `group create\|list\|invite\|join\|add\|members` |
| DX | `jobs` `logs` `config` `update` `completion` |
| Ops | `sync` `dashboard` |

**Hybrid apps** (project on your disk, process on the peer):

```bash
gpumesh app run --peer alice-pc --dir ./train python train.py
```

**Public discovery** (metadata only — still pair before `run`):

```bash
gpumesh config set rendezvous_url http://<control-plane>:8080
gpumesh share --public --region us-west
gpumesh search --gpu 4090 --idle
```

---

## Security model

- **Default deny** — providers only accept peers on the allowlist (pairing auto-allows; use `gpumesh deny`)
- **No host shell** — remote jobs run in Docker with GPU access, not an unrestricted login
- **Signed identity** — Ed25519 keys; pairing and public listings are signed
- **Control plane** — rendezvous + dashboard metadata; optional `GPUMESH_API_TOKEN`; does not execute GPU containers

Report security issues privately if you find them; do not open public issues for exploitable flaws until coordinated.

---

## Repository layout

```text
crates/          Rust workspace (CLI, agent, protocol, control plane, relay, …)
dashboard/       Next.js local operations console
services/        Optional Go control-plane (legacy / alternate)
docs/            Guides and design notes
scripts/         Install helpers
PRD.md           Product requirements & roadmap
```

---

## Documentation

| Doc | Topic |
| --- | --- |
| [PRD.md](./PRD.md) | Product vision & phases |
| [docs/two-machine-demo.md](./docs/two-machine-demo.md) | Two-laptop walkthrough |
| [docs/phase4-dx.md](./docs/phase4-dx.md) | DX (jobs, logs, install) |
| [docs/phase5-6.md](./docs/phase5-6.md) | Groups & dashboard |
| [docs/phase7.md](./docs/phase7.md) | Public registry |
| [docs/gpu-desktop.md](./docs/gpu-desktop.md) | RDP/VNC over QUIC |
| [docs/cuda-remote.md](./docs/cuda-remote.md) | CUDA remoting spike |
| [docs/research.md](./docs/research.md) | Research notes |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | How to contribute |

---

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](./CONTRIBUTING.md) before opening a PR.

```bash
cargo build -p gpumesh-cli -p gpumesh-control
cargo test -p gpumesh-core
cd dashboard && npm install && npm run build
```

---

## License

GPUMesh is licensed under the [Apache License 2.0](./LICENSE).

```text
Copyright GPUMesh Contributors
```

---

## Acknowledgements

Built for researchers, students, and small teams who already have GPUs — and would rather share them than rent them for every experiment.
