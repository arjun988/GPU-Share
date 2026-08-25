# GPUMesh

**Turn idle GPUs into a personal compute network.**

Phases **0–6** implemented: P2P GPU sharing, DX, private clusters, and local dashboard.

## Install

```bash
GPUMESH_FROM_SOURCE=1 ./scripts/install.sh
# or
cargo install --path crates/gpumesh-cli
```

On each laptop you need: Docker, an NVIDIA GPU + [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) (on machines that **share** GPUs), and the `gpumesh` binary on `PATH`.

## Quick start (single machine)

```bash
gpumesh start                 # interactive menu (arrow keys)
# or:
gpumesh init --name alice-pc
gpumesh doctor
gpumesh share
```

## Connect two laptops (2 users)

Pairing is **mutual and explicit** — each machine must allow the other. Codes include identity + address hints; machines must be able to reach each other over the network (same LAN is the simple case).

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

**Mutual allow** — A must also pair B. On B run `gpumesh pair-code` (or start `gpumesh share` briefly) and copy B’s code; on A:

```bash
gpumesh pair '<bob-pairing-code>'
```

**Run a job from B on A’s GPU**

```bash
# B keeps Alice sharing; then on Bob:
gpumesh run --peer alice-pc --image python:3.12-slim nvidia-smi
# or any command / script with --workdir / path args
gpumesh jobs
```

If QUIC cannot connect directly (strict NAT / different networks), set a relay when available:

```bash
export GPUMESH_RELAY=host:port
```

## Connect N laptops (private cluster)

Everyone still **pairs** with the peers they will talk to (or at least with the group owner and providers). Then use a **group** so `run --group` can schedule onto an idle GPU.

**Owner (any trusted laptop)**

```bash
gpumesh group create research
gpumesh group invite research          # share this invite code with the team
```

**Each other laptop**

```bash
gpumesh group join '<invite-code>'
# Pair with the owner (and with any machine you will run jobs on / share from):
gpumesh pair '<their-pairing-code>'
```

**Add already-paired peers by name** (on a machine that knows them):

```bash
gpumesh group add research alice-pc
gpumesh group add research bob-laptop
gpumesh group members research
```

**Providers** leave sharing on:

```bash
gpumesh share
```

**Anyone in the group** can schedule:

```bash
gpumesh run --group research --gpu-memory 8GB --image python:3.12-slim python train.py
```

The scheduler probes group members, skips busy / low-VRAM peers, and picks the best idle GPU.

### Mental model

| Step | What it does |
| --- | --- |
| `init` | Creates identity under `~/.gpumesh` |
| `pair` | Trust + allowlist between two nodes |
| `share` | Provider listens and accepts jobs from allowed peers |
| `group` | Named set of members for cluster scheduling |
| `run --peer` | Pin a job to one machine |
| `run --group` | Auto-pick an idle member with enough free VRAM |

Same Wi‑Fi / LAN works with the addresses inside pairing codes. Across the internet you need reachable IPs, port forwarding for the listen port (default `47000` UDP), or `GPUMESH_RELAY`.

## Dashboard (optional, local)

Metadata only — does not run jobs.

```bash
# API (Rust control plane works well from the same host as the CLI)
cargo run -p gpumesh-control
# or: cd services/control-plane && go run .

# UI
cd dashboard && npm install && npm run dev

gpumesh config set rendezvous_url http://127.0.0.1:8080
gpumesh sync
gpumesh dashboard
```

Open http://127.0.0.1:3000 (or the port Next prints).

## CLI

| Area | Commands |
| --- | --- |
| Core | `start` `init` `status` `gpu` `doctor` `share` `pair-code` `pair` `peers` `run` |
| Clusters | `group create\|list\|invite\|join\|add\|members` |
| DX | `jobs` `logs` `config` `update` `completion` |
| Cloud | `sync` `dashboard` |

## Layout

```text
crates/      Rust CLI + agent + protocol + control plane
services/    Go control-plane (dashboard API)
dashboard/   Next.js + Tailwind UI
docs/
```

Docs: [PRD](./PRD.md) · [Two-machine demo](./docs/two-machine-demo.md) · [Phase 4](./docs/phase4-dx.md) · [Phases 5–6](./docs/phase5-6.md)
