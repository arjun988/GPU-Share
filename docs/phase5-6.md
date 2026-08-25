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

```bash
# Terminal A — API
cd services/control-plane && go run .

# Terminal B — UI
cd dashboard && npm install && npm run dev

# Terminal C — sync node metadata
gpumesh config set rendezvous_url http://127.0.0.1:8080
gpumesh sync
gpumesh dashboard   # prints URLs
```

Open http://127.0.0.1:3000

Pages: Overview, My GPUs, Peers, Jobs, Network, Usage, Settings, Security.

Control plane never executes workloads — metadata only (`/v1/sync`, `/v1/overview`, …).
