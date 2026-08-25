# Architecture (Phases 0–7)

```text
gpumesh (CLI) ──QUIC──► gpumesh agent
                           │
                           ├── identity / allowlist
                           ├── job manager
                           └── Docker + NVIDIA GPU
```

## Data plane

- Transport: Quinn QUIC + TLS 1.3 (ALPN `gpumesh/1`)
- Application auth: fresh, signed Ed25519 Hello / HelloAck (TLS verifier is intentionally permissive; trust is at the app layer + allowlist)
- Framing: `[u32 BE length][JSON Message]`
- WAN fallback: the `gpumesh-relay` crate relays encrypted peer sessions when direct dialing fails

## Control plane

`gpumesh-control` is the primary optional HTTP rendezvous for peer metadata. Public
registry listings are signed, freshness-checked, and omit peer addresses. The
control plane must never receive workload bytes.

## Local state

`~/.gpumesh/`:

- `identity.json` — Ed25519 seed + node id
- `config.toml` — name, port, limits, rendezvous URL
- `peers.json` — paired peers
- `allowlist.json` — allow / deny
- `jobs/` — packaged workloads and workspaces
- `work/` — inbound file transfers

GPUMesh source and binaries are distributed under Apache-2.0.
