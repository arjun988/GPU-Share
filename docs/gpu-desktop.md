# GPU desktop — current product vs remote desktop

GPUMesh supports **three modes**:

1. **Jobs (scripts)** — `gpumesh run` / `cp` (Phases 0–7)  
2. **Desktop (apps)** — `gpumesh desktop` tunnels RDP/VNC so you can use the host GPU interactively  
3. **Hybrid launcher** — `gpumesh app` packages a local project, runs it on the peer, and pulls outputs

---

## Quick start (2 users)

### Host (GPU machine)

1. Enable a desktop backend on the host OS:
   - **Windows:** Settings → System → Remote Desktop → **On** (port **3389**)
   - **Linux:** run VNC (e.g. `wayvnc` / `x11vnc`) on **5900**
2. Check: `gpumesh desktop doctor`
3. Share:

```bash
gpumesh init --name alice-pc
gpumesh desktop share          # leave running; copy pairing code if needed
```

4. After the client pairs, allow desktop:

```bash
gpumesh desktop allow bob-laptop
```

### Client

```bash
gpumesh init --name bob-laptop
gpumesh pair '<alice-pairing-code>'
# Alice must also pair Bob (mutual), then:
gpumesh desktop connect alice-pc
```

Then open the viewer to the printed local address:

```text
Windows RDP:  mstsc /v:127.0.0.1:13389
VNC:          any VNC client → 127.0.0.1:15900
```

You now use **Alice’s desktop/GPU apps** (Blender GUI, browsers, IDEs, etc.) over an authenticated GPUMesh tunnel.

### Scripts on the same host

Desktop allow also grants job allow:

```bash
gpumesh run --peer alice-pc --workdir ./train python train.py
gpumesh cp ./data.bin alice-pc:/data.bin
```

---

## What we have now

### Jobs / scripts

| Capability | Status |
| --- | --- |
| Pairing + allowlist + signed Hello | Done |
| Docker GPU jobs + file `cp` | Done |
| Groups + scheduler | Done |
| Public registry (metadata) | Done |

### Desktop / apps

| Capability | Status |
| --- | --- |
| `gpumesh desktop share` | Done |
| `gpumesh desktop connect <peer>` | Done |
| `gpumesh desktop allow <peer>` | Done (separate from job-only allow) |
| `gpumesh desktop doctor` | Done |
| Auto-detect RDP (:3389) / VNC (:5900+) | Done |
| QUIC TCP tunnel (multi-connection) | Done |
| Scripts still work via `run` | Done |

---

## How it works

```text
Client                         Host
  desktop connect ──QUIC──►  desktop share
       │                         │
  listen 127.0.0.1:13389    localhost:3389 (RDP)
       │                         │
  mstsc/VNC viewer          Windows/Linux desktop + GPU apps
```

- GPUMesh does **auth + P2P tunnel** (not pixel codecs).
- The **OS remote desktop** (RDP/VNC) provides display + input + “any app”.
- Control plane never sees desktop pixels.

---

## Security model

| Rule | Behavior |
| --- | --- |
| Job allow | `gpumesh allow` / auto on pair — Docker jobs only |
| Desktop allow | `gpumesh desktop allow` — required for desktop tunnel |
| Deny | `gpumesh deny` clears both |
| Backend | Must already be listening on host localhost |

Desktop is a **higher privilege** than jobs: full interactive access to the host session exposed by RDP/VNC.

---

## Limits / known gaps

| Item | Notes |
| --- | --- |
| WAN quality | Depends on RDP/VNC + network; use relay/`GPUMESH_RELAY` if needed |
| Sunshine/Moonlight UDP | Not fully supported yet (TCP helper only); prefer RDP for apps |
| Path B (local app → remote CUDA) | Spike: **`gpumesh cuda`** (narrow API); not drop-in ICD |
| Consent UI / idle timeout | Future (D2) |
| Custom WebRTC encoder | Future; not required while RDP/VNC works |

---

## CLI reference

```bash
gpumesh desktop doctor
gpumesh desktop share
gpumesh desktop allow <peer>
gpumesh desktop connect <peer>
gpumesh desktop connect <peer> --port 13389
```

Also in `gpumesh start` under **Desktop**.

Hybrid launcher (project stays local; process runs on the peer):

```bash
gpumesh app sync --peer alice-pc --dir ./proj
gpumesh app run --peer alice-pc --dir ./train python train.py
gpumesh app run --peer alice-pc --dir ./blend --out ./renders --desktop blender -b scene.blend -a
gpumesh app pull --peer alice-pc --job <id> --dir ./out
```

Also in `gpumesh start` under **Apps (hybrid)**.

---

## Related

- [Research: local apps on remote GPU](./research.md) — Path B research; R1=`app`, R2=`cuda`
- [CUDA remoting](./cuda-remote.md) — R2 spike
- [PRD](../PRD.md)
- [Two-machine demo](./two-machine-demo.md) — job-only flow
- [Architecture](./architecture.md)
