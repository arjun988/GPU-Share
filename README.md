# GPUMesh

Turn idle NVIDIA GPUs into a private compute network.

GPUMesh lets people you trust run Docker GPU jobs on your machine — and lets you run jobs on theirs — over authenticated peer-to-peer connections. No SSH, no VPN, no cloud that sees your workload. Pair once, allowlist stays default-deny, jobs stay in containers.

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-alpha-yellow.svg)](#status)

```bash
# Provider (has the GPU)
gpumesh init --name alice-pc
gpumesh share

# Consumer
gpumesh init --name bob-laptop
gpumesh pair '<alice-pairing-code>'
gpumesh run --peer alice-pc nvidia-smi
```

---

## What it is

GPUMesh is a **CLI-first** product with an optional **local dashboard**.

| You want | You use |
| --- | --- |
| Run a command on a friend’s GPU | `gpumesh run --peer <name> …` |
| Train from a local folder, execute remotely | `gpumesh app run --peer <name> --dir . python train.py` |
| Share your GPU | `gpumesh share` |
| Trust a new machine | Mutual `gpumesh pair` |
| A team of GPUs | `gpumesh group` + `run --group` |
| Live metrics, logs, pair/connect from a browser | Local dashboard |
| Find public listings | `gpumesh search` (metadata only — still pair before jobs) |

It is **not** cloud GPU rental, a game streaming service, or “attach their GPU as a local CUDA device” for PyTorch. The process runs **on the peer**, next to their GPU, inside Docker (unless you opt into the narrower desktop / CUDA remoting tools).

---

## Status

**Alpha (0.1).** The core loop works: identity, pairing, sharing, jobs, groups, dashboard, optional public registry, relay. CLI flags and protocol details can still change. There is no marketplace, billing, or reputation system.

---

## Features

- **Ed25519 node identity** stored under `~/.gpumesh`
- **Mutual pairing** with signed codes (identity + address hints)
- **Default-deny allowlist** (`allow` / `deny`; pairing auto-allows for UX)
- **QUIC P2P** (default UDP `47000`); optional **relay** when NAT blocks direct dial
- **Docker + NVIDIA** jobs with image, env, workdir, VRAM hints, retries, logs
- **Groups** with invite codes and idle / free-VRAM scheduling
- **File copy** `gpumesh cp` and sandboxed `gpumesh exec` (container, not host SSH)
- **Local dashboard** — GPUs, peers, jobs, logs, pair, connect, run (light / dark)
- **Public registry** — signed GPU metadata only; workloads never go through it
- **Desktop tunnel** — RDP/VNC over the same authenticated connection
- **CUDA remoting** — LAN Runtime-API subset for experiments (not a PyTorch ICD)

---

## Requirements

| Role | Required |
| --- | --- |
| Build / CLI | [Rust](https://rustup.rs/) (stable), Git |
| Provider (shares GPU) | NVIDIA GPU + driver, [Docker](https://docs.docker.com/get-docker/), [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html) |
| Consumer | Network path to the provider (same LAN is the simple case) |
| Dashboard | Node.js 18+ |

**Platforms:** Linux + NVIDIA is the primary target. Windows and WSL work for many flows. macOS cannot share NVIDIA GPUs.

---

## Install

```bash
git clone https://github.com/gpumesh/gpumesh.git
cd gpumesh

GPUMESH_FROM_SOURCE=1 ./scripts/install.sh
# or
cargo install --path crates/gpumesh-cli
```

Put `~/.local/bin` or `$HOME/.cargo/bin` on `PATH`, then:

```bash
gpumesh --help
gpumesh doctor
```

---

## Quick start

State lives in **`~/.gpumesh`** (identity, config, peers, jobs, logs).

### One machine

```bash
gpumesh start                 # interactive menu
# or
gpumesh init --name alice-pc
gpumesh doctor
gpumesh share                 # leave running to accept jobs
```

Stop sharing:

```bash
gpumesh share stop
```

### Two machines

Pairing is **mutual**. Both machines must reach each other.

**A — provider**

```bash
gpumesh init --name alice-pc
gpumesh doctor
gpumesh share                 # copy the pairing code; leave this running
```

**B — consumer**

```bash
gpumesh init --name bob-laptop
gpumesh pair '<alice-pairing-code>'
gpumesh peers
```

**A must pair B as well.** On B: `gpumesh pair-code`. On A:

```bash
gpumesh pair '<bob-pairing-code>'
```

**Run from B on A’s GPU**

```bash
gpumesh run --peer alice-pc --image python:3.12-slim nvidia-smi
gpumesh jobs
gpumesh logs <job-id>
```

### Different networks / strict NAT

```bash
# On a host with a public UDP port (default 4799)
cargo run -p gpumesh-relay

# On each GPUMesh node
export GPUMESH_RELAY=host:4799
```

You can also forward UDP **47000** (listen port) instead of using a relay.

### Private cluster

```bash
gpumesh group create research
gpumesh group invite research          # share invite with the team
gpumesh group join '<invite-code>'
gpumesh pair '<peer-code>'             # still pair with machines you talk to
gpumesh group add research alice-pc

# Providers
gpumesh share

# Anyone in the group
gpumesh run --group research --gpu-memory 8GB python train.py
```

The scheduler probes members, skips busy / low-VRAM nodes, and picks an idle GPU.

### Local dashboard

```bash
# Terminal 1 — API (this machine’s ~/.gpumesh + live NVML)
cargo run -p gpumesh-control

# Terminal 2 — UI
cd dashboard && npm install && npm run dev
```

Open **http://127.0.0.1:3000**. `gpumesh dashboard` prints the URLs.

From the UI you can view GPUs and job logs, generate/paste pair codes, allow/deny, connect, and start a remote run. Optional: `GPUMESH_API_TOKEN` on the API and `NEXT_PUBLIC_GPUMESH_API_TOKEN` on the UI.

---

## How it works

```text
Consumer                         Provider
┌─────────┐   Ed25519 + QUIC    ┌─────────────────┐
│ gpumesh │ ─────────────────►  │ gpumesh share   │
│ run/app │   allowlisted only  │  Docker + GPU   │
└─────────┘                     └─────────────────┘

Control plane / dashboard = metadata + local ops
Workload bytes stay peer-to-peer
```

| Step | What happens |
| --- | --- |
| `init` | Create keys and config in `~/.gpumesh` |
| `pair` | Store peer identity + addresses; allow jobs |
| `share` | Listen; accept jobs only from the allowlist |
| `run --peer` | Pack workdir, dial peer, run container, stream logs |
| `run --group` | Probe group, pick idle GPU with enough VRAM |
| `deny` | Revoke access (jobs, desktop, CUDA remoting) |

---

## Commands

```text
gpumesh start                 Interactive menu
gpumesh init [--name NAME]
gpumesh doctor
gpumesh status | gpu
gpumesh share [--max-vram 16GB] [--public] [--region us-west]
gpumesh share stop
gpumesh pair-code | pair <code> | peers | connect <peer>
gpumesh allow <peer> | deny <peer>
gpumesh run [--peer NAME] [--group NAME] [--gpu-memory 8GB]
            [--image IMG] [--workdir DIR] [-f job.yaml] [--retries N]
            [--env KEY=VAL] -- <command>
gpumesh jobs [--limit 20]
gpumesh logs [JOB_ID] [-f]
gpumesh cancel --peer NAME JOB_ID
gpumesh cp SRC DST                 # local or peer:path
gpumesh exec PEER [shell]
gpumesh group create|list|invite|join|add|members
gpumesh app sync|run|pull
gpumesh desktop share|connect|allow|doctor
gpumesh cuda share|allow|demo|bench|bridge|doctor
gpumesh search [--gpu 4090] [--vram 8GB] [--idle]
gpumesh config show|get|set|path
gpumesh dashboard | sync
gpumesh completion bash|zsh|fish|powershell
```

### Jobs

```bash
gpumesh run --peer alice-pc --image nvidia/cuda:12.8.0-runtime-ubuntu22.04 nvidia-smi
gpumesh run --peer alice-pc --workdir ./train --env WANDB_MODE=offline python train.py
gpumesh run --group research --gpu-memory 20GB python train.py
gpumesh run -f job.yaml
gpumesh logs <id> --follow
gpumesh cancel --peer alice-pc <id>
```

Default image: `nvidia/cuda:12.8.0-runtime-ubuntu22.04` (override with `--image` or `GPUMESH_IMAGE`).

### Apps (local project, remote process)

The directory is packed (respects `.gpumeshignore`), uploaded, and executed **on the peer**. After the job, outputs can be pulled back. This is not local-app CUDA remoting.

```bash
gpumesh app sync --peer alice-pc --dir ./proj
gpumesh app run --peer alice-pc --dir ./train python train.py
gpumesh app run --peer alice-pc --dir ./blend --out ./renders blender -b scene.blend -a
gpumesh app pull --peer alice-pc --job <id> --dir ./out
```

### Files and exec

```bash
gpumesh cp ./dataset.bin alice-pc:/dataset.bin
gpumesh cp alice-pc:/out.bin ./out.bin
gpumesh exec alice-pc bash          # container shell, not host SSH
```

### GPU desktop

RDP (`:3389`) or VNC (`:5900`) tunneled over the authenticated connection. Separate desktop allowlist.

```bash
# Host: enable Remote Desktop / VNC, then
gpumesh desktop share
gpumesh desktop allow bob-laptop

# Client
gpumesh desktop connect alice-pc
# Windows: mstsc /v:127.0.0.1:13389
```

### CUDA remoting

LAN Runtime-API subset. Real driver path when `libcuda` loads; otherwise host-memory fallback. Not a drop-in for PyTorch.

```bash
gpumesh cuda share
gpumesh cuda allow bob-laptop
gpumesh cuda demo --peer alice-pc
gpumesh cuda bench --peer alice-pc
gpumesh cuda bridge --peer alice-pc --bind 127.0.0.1:17999
```

### Public listings

Publishing **does not** open your GPU to strangers. Search is metadata. You still pair before `run`.

```bash
gpumesh config set rendezvous_url http://<control-plane>:8080
gpumesh share --public --region us-west
gpumesh search --gpu 4090 --vram 8GB --idle
```

---

## Configuration

```bash
gpumesh config path
gpumesh config show
gpumesh config set listen_port 47000
gpumesh config set rendezvous_url http://127.0.0.1:8080
```

| Variable | Purpose |
| --- | --- |
| `GPUMESH_NODE_NAME` | Default node name on `init` |
| `GPUMESH_PEER` | Default `--peer` |
| `GPUMESH_IMAGE` | Default Docker image |
| `GPUMESH_RELAY` | Relay `host:port` |
| `GPUMESH_REGION` | Public listing region |
| `GPUMESH_API_TOKEN` | Bearer token for control plane |
| `GPUMESH_API_ADDR` | Control plane bind (default `0.0.0.0:8080`) |
| `GPUMESH_LOG` / `RUST_LOG` | Log filter |

---

## Security

- Providers accept jobs only from the **allowlist**.
- Remote users do **not** get a host shell. Path: authenticated P2P → sandbox → Docker → NVIDIA GPU.
- Pairing codes and public announces are **signed**. Pairing codes expire (1 hour).
- Control plane stores **metadata**. Optional `GPUMESH_API_TOKEN`.
- Dashboard connect/run uses an **ephemeral QUIC dialer** so it does not steal the share port.

Report vulnerabilities privately. Do not file public issues with exploit details.

---

## Repository

```text
crates/gpumesh-cli         CLI (`gpumesh`)
crates/gpumesh-agent       Provider agent
crates/gpumesh-core        Pairing, jobs, desktop, CUDA remoting
crates/gpumesh-control     Dashboard / rendezvous API (`:8080`)
crates/gpumesh-relay       NAT fallback
crates/gpumesh-protocol    Wire protocol
crates/gpumesh-network     QUIC + discovery
crates/gpumesh-security    Identity, allowlist, signatures
crates/gpumesh-runtime     Docker
crates/gpumesh-gpu         NVML / nvidia-smi
dashboard/                 Next.js console
scripts/                   Installer
```

```bash
cargo build -p gpumesh-cli -p gpumesh-agent -p gpumesh-control
cargo test -p gpumesh-core
cd dashboard && npm install && npm run build
```

See [CONTRIBUTING.md](./CONTRIBUTING.md).

---

## License

[Apache License 2.0](./LICENSE).
