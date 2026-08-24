# Two-machine demo (Phases 1–3)

## Prerequisites

- Linux hosts with NVIDIA GPU on the **provider**
- Docker + [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html)
- Rust toolchain (to build)

## Build

```bash
cargo build --release -p gpumesh-cli -p gpumesh-agent
export PATH="$PWD/target/release:$PATH"
```

## Provider

```bash
gpumesh init --name alice-pc
gpumesh share --max-vram 16GB
# prints pairing code; leave running
```

## Consumer

```bash
gpumesh init --name bob-laptop
gpumesh pair '<pairing-code-from-alice>'
gpumesh peers
gpumesh run --peer alice-pc --image nvidia/cuda:12.8.0-runtime-ubuntu22.04 nvidia-smi
```

## File copy

```bash
gpumesh cp ./dataset.bin alice-pc:/dataset.bin
gpumesh cp alice-pc:/dataset.bin ./roundtrip.bin
```

## Notes

- Workloads run inside Docker with `--gpus all` — never as host shell.
- Default deny: pairing auto-allows the peer; use `gpumesh deny <peer>` to revoke.
- Optional relay: `export GPUMESH_RELAY=host:port` when direct QUIC fails.
- Optional rendezvous: set `rendezvous_url` in `~/.gpumesh/config.toml` and run `services/control-plane`.
