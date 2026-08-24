# Contributor guide

## Scope

Phases 0–3 are the current build contract (see `PRD.md`). Prefer fixing/extending those before Phase 4+ features.

## Layout

Rust workspace under `crates/`. Optional rendezvous service under `services/control-plane`.

## Conventions

- Workloads must stay containerized; no host shell for remote users.
- Control plane is signaling/metadata only — never run jobs there.
- Protocol messages live in `gpumesh-protocol` and should stay backward compatible within major version 1.

## Development

```bash
cargo build -p gpumesh-cli
cargo build -p gpumesh-agent
```
