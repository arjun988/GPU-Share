# GPUMesh

**Turn idle GPUs into a personal compute network.**

```bash
gpumesh init
gpumesh share
gpumesh pair <code>
gpumesh run --peer alice python train.py
```

Open-source, CLI-first P2P GPU compute for trusted peers. Phases **0–4** implemented.

## Install

```bash
# from this repo
GPUMESH_FROM_SOURCE=1 ./scripts/install.sh

# or
cargo install --path crates/gpumesh-cli
export PATH="$HOME/.cargo/bin:$PATH"
```

Shell completions:

```bash
source <(gpumesh completion bash)   # or zsh / fish / powershell
```

## Quick start

```bash
gpumesh init --name alice-pc
gpumesh doctor
gpumesh share

# other machine / other GPUMESH_HOME
gpumesh init --name bob
gpumesh pair '<code>'
gpumesh run --peer alice-pc --image python:3.12-slim echo hello
```

YAML jobs:

```bash
gpumesh run --file examples/job.yaml
```

## CLI

| Command | Description |
| --- | --- |
| `init` / `status` / `gpu` / `doctor` | Setup & diagnostics |
| `share` / `pair` / `peers` / `connect` | Private P2P network |
| `run` / `jobs` / `logs` / `cancel` | Jobs |
| `cp` / `exec` | Files & isolated shell |
| `config` / `update` / `completion` | DX (Phase 4) |

Environment: `GPUMESH_HOME`, `GPUMESH_PEER`, `GPUMESH_IMAGE`, `GPUMESH_LOG`.

## Layout

```text
crates/gpumesh-cli/     # polished `gpumesh` binary
crates/gpumesh-agent/   # provider daemon
crates/gpumesh-*        # core protocol/runtime/network
services/control-plane/ # optional rendezvous
scripts/install.sh
docs/
```

## Docs

- [PRD](./PRD.md)
- [Phase 4 DX](./docs/phase4-dx.md)
- [Two-machine demo](./docs/two-machine-demo.md)
- [Completions](./docs/completions.md)

## License

Apache-2.0 recommended (see PRD).
