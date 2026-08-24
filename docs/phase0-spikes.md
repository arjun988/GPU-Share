# Phase 0 — Research & Architecture Spikes

Goal: validate the hardest assumptions behind GPUMesh before productizing.

The production crates under `crates/` already incorporate these spikes:

| Spike | Location |
| --- | --- |
| GPU detection (NVML / nvidia-smi) | `crates/gpumesh-gpu` |
| Docker GPU execution | `crates/gpumesh-runtime` |
| QUIC connection | `crates/gpumesh-network` (Quinn) |
| Peer identity (Ed25519) | `crates/gpumesh-security` |
| NAT / relay fallback | `crates/gpumesh-network/src/relay.rs` |
| File transfer | `crates/gpumesh-core/src/remote.rs` + protocol frames |
| Remote log streaming | `Message::JobLog` over QUIC streams |
| LAN discovery | mDNS in `gpumesh-network` |
| Rendezvous signaling | `services/control-plane` (HTTP, no workloads) |

## Target demo path

```text
Machine A (consumer CLI)
        │  QUIC P2P
        ▼
Machine B (gpumesh share / agent)
        │
        ▼
     Docker + NVIDIA Container Toolkit
        │
        ▼
     RTX GPU
```

No dashboard, marketplace, or accounts in this phase.
