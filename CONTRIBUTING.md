# Contributing to GPUMesh

Thanks for helping improve GPUMesh. This project is CLI-first, Apache-2.0, and built around a simple rule: **remote workloads stay sandboxed — never an unrestricted host shell.**

## Before you start

1. Read the [README](./README.md) for product intent and commands.
2. Keep the control plane / dashboard **metadata + local ops only** — do not run GPU containers there.
3. Prefer focused changes over large new surfaces.

## Development setup

```bash
cargo build -p gpumesh-cli -p gpumesh-agent
cargo build -p gpumesh-control
cargo test -p gpumesh-core
cd dashboard && npm install && npm run build
```

On Linux providers you also need Docker and the NVIDIA Container Toolkit to exercise `share` / `run` end-to-end.

## Conventions

- **Protocol:** message types live in `gpumesh-protocol`. Stay backward compatible within major version `1`.
- **Security:** default-deny allowlists; pairing and public listings are signed; do not weaken auth “for demos.”
- **Honesty:** document limits (desktop, CUDA remoting, WAN) clearly.
- **Style:** match surrounding Rust / TypeScript; keep diffs focused.

## Pull requests

- One concern per PR when practical.
- Describe **why**, how you tested (`cargo test`, two-machine smoke, dashboard), and any user-facing CLI changes.
- Do not commit secrets, `.env` files, or large binaries.

## Security reports

If you find a vulnerability, contact the maintainers privately rather than opening a public issue with exploit details.

## License

By contributing, you agree that your contributions are licensed under the [Apache License 2.0](./LICENSE).
