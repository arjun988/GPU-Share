# Architecture (Phases 0–3)

```text
gpumesh (CLI) ──QUIC──► gpumesh agent
                           │
                           ├── identity / allowlist
                           ├── job manager
                           └── Docker + NVIDIA GPU
```

## Data plane

- Transport: Quinn QUIC + TLS 1.3 (ALPN `gpumesh/1`)
- Application auth: Ed25519 Hello / HelloAck (TLS verifier is intentionally permissive; trust is at the app layer + allowlist)
- Framing: `[u32 BE length][JSON Message]`

## Control plane

`services/control-plane` is optional HTTP rendezvous for peer metadata. It must never receive workload bytes.

## Local state

`~/.gpumesh/`:

- `identity.json` — Ed25519 seed + node id
- `config.toml` — name, port, limits, rendezvous URL
- `peers.json` — paired peers
- `allowlist.json` — allow / deny
- `jobs/` — packaged workloads and workspaces
- `work/` — inbound file transfers
