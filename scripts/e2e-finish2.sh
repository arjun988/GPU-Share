#!/usr/bin/env bash
set -eu
ROOT=/mnt/c/Users/Arjun/Desktop/GPU-Share
GM="$ROOT/target/debug/gpumesh"
CTRL="$ROOT/target/debug/gpumesh-control"
export PATH="$HOME/.cargo/bin:$PATH"

# Prefer linux node, else windows node via npm.cmd path
if ! command -v node >/dev/null 2>&1; then
  export PATH="/mnt/c/Program Files/nodejs:$PATH"
fi
echo "node=$(command -v node || true)"
echo "npm=$(command -v npm || true)"

# Ensure API up
if ! curl -fsS http://127.0.0.1:8080/healthz >/dev/null 2>&1; then
  fuser -k 8080/tcp 2>/dev/null || true
  nohup "$CTRL" > /tmp/gpumesh-api.log 2>&1 &
  sleep 1
fi
curl -fsS http://127.0.0.1:8080/healthz; echo

# Dashboard
cd "$ROOT/dashboard"
if [ ! -d node_modules ]; then
  npm install
fi
pkill -f "next dev" 2>/dev/null || true
nohup npm run dev -- --hostname 127.0.0.1 --port 3000 > /tmp/gpumesh-dash.log 2>&1 &
for i in $(seq 1 45); do
  if curl -fsS -o /dev/null http://127.0.0.1:3000/; then
    echo DASH_OK
    break
  fi
  sleep 1
done
tail -30 /tmp/gpumesh-dash.log
curl -fsS -o /dev/null -w "DASH:%{http_code}\n" http://127.0.0.1:3000/ || true

# Ensure alice/bob state
if [ ! -d /tmp/gm-alice/.gpumesh ]; then
  mkdir -p /tmp/gm-alice /tmp/gm-bob
  HOME=/tmp/gm-alice "$GM" init --name alice
  HOME=/tmp/gm-bob "$GM" init --name bob
  printf '%s\n' 'node_name = "bob"' 'listen_port = 47001' 'default_image = "python:3.12-slim"' 'max_concurrent_jobs = 1' 'sharing_enabled = false' 'default_retries = 0' > /tmp/gm-bob/.gpumesh/config.toml
  HOME=/tmp/gm-bob "$GM" pair-code > /tmp/bob-code.log 2>&1
  HOME=/tmp/gm-alice "$GM" pair-code > /tmp/alice-code.log 2>&1
  HOME=/tmp/gm-alice "$GM" pair "$(grep -E '^eyJ' /tmp/bob-code.log | head -1)"
  HOME=/tmp/gm-bob "$GM" pair "$(grep -E '^eyJ' /tmp/alice-code.log | head -1)"
  HOME=/tmp/gm-alice "$GM" group create research || true
  HOME=/tmp/gm-alice "$GM" group add research bob || true
fi

if ! pgrep -f "target/debug/gpumesh share" >/dev/null; then
  HOME=/tmp/gm-alice nohup "$GM" share > /tmp/alice-share.log 2>&1 &
  sleep 2
fi

INV=$(HOME=/tmp/gm-alice "$GM" group invite research 2>/dev/null | grep -E '^[A-Za-z0-9_-]{40,}$' | head -1 || true)
if [ -n "$INV" ]; then
  HOME=/tmp/gm-bob "$GM" group join "$INV" || true
fi

echo "==== CONNECT + RUN ===="
HOME=/tmp/gm-bob "$GM" connect alice
timeout 120 env HOME=/tmp/gm-bob "$GM" run --peer alice --workdir /tmp --image python:3.12-slim echo hello-final
echo RUN_EC:$?

echo "==== SCHEDULE ===="
timeout 120 env HOME=/tmp/gm-bob "$GM" run --group research --gpu-memory 1GB --workdir /tmp --image python:3.12-slim echo scheduled-ok
echo SCHED_EC:$?

echo "==== SYNC ===="
HOME=/tmp/gm-alice "$GM" config set rendezvous_url http://127.0.0.1:8080
HOME=/tmp/gm-alice "$GM" sync
curl -fsS http://127.0.0.1:8080/v1/overview; echo
curl -fsS http://127.0.0.1:8080/v1/gpus; echo
curl -fsS http://127.0.0.1:8080/v1/jobs; echo
curl -fsS http://127.0.0.1:8080/v1/groups; echo
echo DONE
