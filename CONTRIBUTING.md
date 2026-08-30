# Contributing to GPUMesh

Thanks for helping improve GPUMesh. This project is CLI-first, Apache-2.0, and built around a simple rule: **remote workloads stay sandboxed — never an unrestricted host shell.**

## Before you start

1. Read the [README](./README.md) and skim [PRD.md](./PRD.md) for product intent.
2. Prefer fixing or extending existing Phases **0–7** behavior over large new surfaces.
3. Keep the control plane / dashboard **metadata + local ops only** — do not run GPU containers there.

## Development setup

```bash
# CLI + agent
cargo build -p gpumesh-cli -p gpumesh-agent

# Local dashboard API
cargo build -p gpumesh-control

# Tests (example)
cargo test -p gpumesh-core

# Dashboard UI
cd dashboard && npm install && npm run build
```

On Linux providers you will also need Docker and the NVIDIA Container Toolkit to exercise `share` / `run` end-to-end.

## Conventions

- **Protocol:** message types live in `gpumesh-protocol`. Stay backward compatible within major version `1`.
- **Security:** default-deny allowlists; pairing and public listings are signed; do not weaken auth “for demos.”
- **Honesty:** document limits (desktop, CUDA remoting, WAN) clearly — no GeForce NOW claims.
- **Style:** match surrounding Rust / TypeScript; keep diffs focused.

## Pull requests

- One concern per PR when practical.
- Describe **why**, how you tested (`cargo test`, two-machine smoke, dashboard), and any user-facing CLI/docs changes.
- Do not commit secrets, `.env` files, or large binaries.

## Security reports

If you find a vulnerability, contact the maintainers privately rather than opening a public issue with exploit details.

## License

By contributing, you agree that your contributions are licensed under the [Apache License 2.0](./LICENSE).
