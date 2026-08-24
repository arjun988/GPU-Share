# GPUMesh

**Turn idle GPUs into a personal compute network.**

Open-source, CLI-first P2P GPU compute for trusted peers. Share an idle NVIDIA GPU; run workloads on a friend’s machine without SSH, VPN, or manual port forwarding.

> Product requirements: see [`PRD.md`](./PRD.md). This tree implements **Phases 0–3**.

## Status (Phases 0–3)

| Phase | Scope | Status |
| --- | --- | --- |
| 0 | Architecture spikes (GPU, Docker, QUIC, transfer, logs) | Implemented in crates |
| 1 | Local agent: `status`, `gpu`, `share`, local `run` | Implemented |
| 2 | Identity, pairing, allow/deny, P2P QUIC, relay fallback | Implemented |
| 3 | Remote `run --peer`, packaging, logs, `cp`, cancel | Implemented |

## Quick start

```bash
# Provider
gpumesh init --name alice-pc
gpumesh share

# Consumer (other machine)
gpumesh init --name bob-laptop
gpumesh pair '<code-from-alice>'
gpumesh run --peer alice-pc --image nvidia/cuda:12.8.0-runtime-ubuntu22.04 nvidia-smi
```

## Workspace layout

```text
crates/
  gpumesh-cli/        # `gpumesh` binary
  gpumesh-agent/      # provider daemon
  gpumesh-core/       # orchestration
  gpumesh-network/    # QUIC P2P, mDNS, relay, rendezvous client
  gpumesh-protocol/   # framed messages
  gpumesh-runtime/    # Docker + NVIDIA GPU jobs
  gpumesh-gpu/        # NVML / nvidia-smi
  gpumesh-security/   # Ed25519 identity + pairing
  gpumesh-storage/    # ~/.gpumesh state + packaging
  gpumesh-common/     # shared types
services/control-plane/  # optional HTTP rendezvous (Go)
docs/
examples/
```

## CLI (MVP)

```bash
gpumesh init
gpumesh status
gpumesh gpu
gpumesh share [--max-vram 16GB] [--max-gpu-utilization 80]
gpumesh share stop
gpumesh pair-code
gpumesh pair <code>
gpumesh peers
gpumesh connect <peer>
gpumesh allow <peer>
gpumesh deny <peer>
gpumesh run [--peer NAME] [--image IMG] [--env K=V] [--workdir DIR] <cmd...>
gpumesh cp <local> <peer>:/path
gpumesh cp <peer>:/path <local>
gpumesh cancel --peer NAME <job-id>
gpumesh exec <peer> [shell]
gpumesh agent --share
```

Config lives in `~/.gpumesh/` (identity, config.toml, peers, allowlist, jobs).

## Security model

```text
Remote user → authenticated P2P → job sandbox → Docker container → GPU
```

Remote peers never receive an unrestricted host shell.

## Stack

- **Rust** + Tokio + Quinn (QUIC) + Ed25519
- **Docker** + NVIDIA Container Toolkit + NVML
- **Go** optional rendezvous (`services/control-plane`)

## Docs

- [PRD](./PRD.md)
- [Phase 0 spikes](./docs/phase0-spikes.md)
- [Two-machine demo](./docs/two-machine-demo.md)

## License

Apache-2.0 recommended (see PRD). Current repo license file may differ until aligned.
