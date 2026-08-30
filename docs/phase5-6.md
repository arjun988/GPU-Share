# Phases 5–6

## Phase 5 — Private GPU clusters

```bash
gpumesh group create research
gpumesh group invite research          # share code
gpumesh group join <code>              # on member machine
gpumesh group add research alice       # add already-paired peer
gpumesh group members research
gpumesh group list

# Scheduler picks an idle peer with enough free VRAM
gpumesh run --group research --gpu-memory 20GB python train.py
gpumesh run --gpu-memory 8GB --image python:3.12-slim echo hello
```

Groups live in `~/.gpumesh/groups.json`. The scheduler probes paired members, filters by sharing/idle + free VRAM, and scores by free memory / utilization.

## Phase 6 — Dashboard

Local operational console (logs, metrics, pair/connect/run) plus optional synced
multi-node metadata.

```bash
# Terminal A — API (Rust; required for /v1/local/*)
cargo run -p gpumesh-control

# Terminal B — UI
cd dashboard && npm install && npm run dev

gpumesh dashboard   # prints URLs
```

Open http://127.0.0.1:3000

Pages: Overview, Connect, GPUs, Peers, Jobs, Logs, Network, Settings, Security.

- **Local** (`/v1/local/*`): live NVML, `~/.gpumesh` peers/jobs/logs, pair, allow,
  connect, run — no sync needed.
- **Synced** (`/v1/sync`, `/v1/overview`, …): optional multi-node metadata store.
  Control plane never runs GPU containers.
