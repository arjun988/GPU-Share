#!/usr/bin/env bash
set -eu
ROOT=/mnt/c/Users/Arjun/Desktop/GPU-Share
GM="$ROOT/target/debug/gpumesh"
CTRL="$ROOT/target/debug/gpumesh-control"
export PATH="$HOME/.cargo/bin:$PATH"
cd "$ROOT"

echo "==== BUILD CONTROL + CLI ===="
cargo build -p gpumesh-control -p gpumesh-cli
test -x "$CTRL"
test -x "$GM"

echo "==== START RUST API ===="
fuser -k 8080/tcp 2>/dev/null || true
pkill -f gpumesh-control 2>/dev/null || true
nohup "$CTRL" > /tmp/gpumesh-api.log 2>&1 &
echo $! > /tmp/gpumesh-api.pid
for i in $(seq 1 30); do
  if curl -fsS http://127.0.0.1:8080/healthz >/dev/null 2>&1; then
    echo API_OK
    break
  fi
  sleep 0.5
done
curl -fsS http://127.0.0.1:8080/healthz
echo

echo "==== DASHBOARD ===="
if ! curl -fsS -o /dev/null http://127.0.0.1:3000/; then
  cd "$ROOT/dashboard"
  if [ ! -d node_modules ]; then npm install; fi
  nohup npm run dev -- --hostname 127.0.0.1 --port 3000 > /tmp/gpumesh-dash.log 2>&1 &
  echo $! > /tmp/gpumesh-dash.pid
  for i in $(seq 1 40); do
    if curl -fsS -o /dev/null http://127.0.0.1:3000/; then break; fi
    sleep 1
  done
fi
curl -fsS -o /dev/null -w "DASH:%{http_code}\n" http://127.0.0.1:3000/

echo "==== ENSURE ALICE SHARE ===="
if ! pgrep -f 'target/debug/gpumesh share' >/dev/null; then
  # recreate nodes if missing
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
  HOME=/tmp/gm-alice nohup "$GM" share > /tmp/alice-share.log 2>&1 &
  sleep 2
fi
pgrep -a 'gpumesh' || true

echo "==== BOB JOIN GROUP ===="
HOME=/tmp/gm-alice "$GM" group invite research > /tmp/invite.log 2>&1 || true
INV=$(grep -E '^[A-Za-z0-9_-]{40,}$' /tmp/invite.log | head -1 || true)
if [ -n "$INV" ]; then
  HOME=/tmp/gm-bob "$GM" group join "$INV" || true
fi
HOME=/tmp/gm-bob "$GM" group list || true

echo "==== CONNECT + RUN ===="
HOME=/tmp/gm-bob "$GM" connect alice
timeout 120 env HOME=/tmp/gm-bob "$GM" run --peer alice --workdir /tmp --image python:3.12-slim echo hello-final
echo RUN_EC:$?

echo "==== SCHEDULED RUN ===="
timeout 120 env HOME=/tmp/gm-bob "$GM" run --group research --gpu-memory 1GB --workdir /tmp --image python:3.12-slim echo scheduled-ok
echo SCHED_EC:$?

echo "==== SYNC + API CHECKS ===="
HOME=/tmp/gm-alice "$GM" config set rendezvous_url http://127.0.0.1:8080
HOME=/tmp/gm-alice "$GM" sync
echo OVERVIEW:
curl -fsS http://127.0.0.1:8080/v1/overview; echo
echo GPUS:
curl -fsS http://127.0.0.1:8080/v1/gpus; echo
echo JOBS:
curl -fsS http://127.0.0.1:8080/v1/jobs; echo
echo GROUPS:
curl -fsS http://127.0.0.1:8080/v1/groups; echo
curl -fsS -o /dev/null -w "DASH:%{http_code}\n" http://127.0.0.1:3000/
echo "==== ALL GOOD ===="
