# GPUMesh

**Turn idle GPUs into a personal compute network.**

Phases **0–6** implemented: P2P GPU sharing, DX, private clusters, and Cloud dashboard.

## Install

```bash
GPUMESH_FROM_SOURCE=1 ./scripts/install.sh
# or
cargo install --path crates/gpumesh-cli
```

## Quick start

```bash
gpumesh init --name alice-pc
gpumesh doctor
gpumesh share

# Phase 5 — private cluster
gpumesh group create research
gpumesh run --group research --gpu-memory 8GB python train.py

# Phase 6 — dashboard
gpumesh config set rendezvous_url http://127.0.0.1:8080
# cd services/control-plane && go run .
# cd dashboard && npm i && npm run dev
gpumesh sync
```

## CLI

| Area | Commands |
| --- | --- |
| Core | `init` `status` `gpu` `doctor` `share` `pair` `run` |
| Clusters | `group create\|list\|invite\|join\|add\|members` |
| DX | `jobs` `logs` `config` `update` `completion` |
| Cloud | `sync` `dashboard` |

## Layout

```text
crates/      Rust CLI + agent + protocol
services/    Go control-plane (dashboard API)
dashboard/   Next.js + Tailwind UI
docs/
```

Docs: [PRD](./PRD.md) · [Phase 4](./docs/phase4-dx.md) · [Phases 5–6](./docs/phase5-6.md)
