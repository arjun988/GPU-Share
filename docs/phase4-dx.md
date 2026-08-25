# Phase 4 — Developer experience

## Commands

| Command | Purpose |
| --- | --- |
| `gpumesh doctor` | Diagnose Docker / NVIDIA / identity / ports |
| `gpumesh config [show\|get\|set\|path]` | Manage `~/.gpumesh/config.toml` |
| `gpumesh jobs` | List recent jobs |
| `gpumesh logs [job-id]` | Show job or agent logs |
| `gpumesh update [--check]` | Version check / binary update |
| `gpumesh completion <shell>` | Shell autocomplete |
| `gpumesh run --file job.yaml` | YAML job definitions |
| `gpumesh run --retries N` | Retry failed runs |

## Install

```bash
curl -fsSL https://install.gpumesh.dev | sh
# or from this repo:
GPUMESH_FROM_SOURCE=1 ./scripts/install.sh
```

## Environment

| Var | Meaning |
| --- | --- |
| `GPUMESH_HOME` | Config root override |
| `GPUMESH_PEER` | Default `--peer` |
| `GPUMESH_IMAGE` | Default `--image` |
| `GPUMESH_LOG` | tracing filter |
| `GPUMESH_NODE_NAME` | Default init name |

## Transfers

Uploads use a resume handshake (`FileAck.resume_from`) and `.partial` files on the provider so interrupted transfers can continue.
