# Phase 7 — Public GPU network

Publish **metadata only** to a shared registry. Listing does **not** authorize jobs — peers must still `pair`.

## Provider

```bash
# Control plane must be reachable
gpumesh config set rendezvous_url http://127.0.0.1:8080
# or: cargo run -p gpumesh-control

gpumesh share --public --region us-west
```

Heartbeats refresh the listing every ~45s. TTL on the registry is ~3 minutes.

## Consumer

```bash
gpumesh search --gpu 4090
gpumesh search --gpu 5060 --vram 8GB --idle --region us
gpumesh search --json
```

Then obtain a pairing code out-of-band and:

```bash
gpumesh pair '<code>'
gpumesh run --peer <name> …
```

## Metadata published

| Field | Meaning |
| --- | --- |
| GPU / VRAM / free VRAM | From local GPU detect |
| CUDA | Driver-reported when available |
| Availability | `idle` / `busy` / `offline` |
| Perf score | Heuristic 0–100 (not pricing) |
| Latency | Search RTT to control plane (approx) |
| Region | Config / `--region` / `GPUMESH_REGION` |
| Uptime | Seconds since this share session started |

## API

| Method | Path |
| --- | --- |
| POST | `/v1/public/announce` |
| POST | `/v1/public/unannounce` |
| GET | `/v1/public/search?gpu=&min_vram_mb=&cuda=&region=&available=` |
| GET | `/v1/public/nodes/{id}` |

No marketplace / payments (Phase 10). Reputation is Phase 8.
