#!/usr/bin/env bash
set -eu
ROOT=/mnt/c/Users/Arjun/Desktop/GPU-Share
GM="$ROOT/target/debug/gpumesh"
CTRL="$ROOT/target/debug/gpumesh-control"
export PATH="$HOME/.cargo/bin:/mnt/c/Program Files/nodejs:$PATH"
cd "$ROOT"

cargo build -p gpumesh-cli -p gpumesh-control

# API
if ! curl -fsS http://127.0.0.1:8080/healthz >/dev/null 2>&1; then
  nohup "$CTRL" > /tmp/gpumesh-api.log 2>&1 &
  sleep 1
fi
echo -n "API: "; curl -fsS http://127.0.0.1:8080/healthz; echo

# Dashboard on 3001 to avoid Windows:3000 conflict
export PATH="/mnt/c/Program Files/nodejs:$PATH"
cd "$ROOT/dashboard"
if [ ! -d node_modules ]; then npm install; fi
# use npx next with windows node - may bind on windows side; try 3001
nohup npm run dev -- --hostname 0.0.0.0 --port 3001 > /tmp/gpumesh-dash.log 2>&1 &
for i in $(seq 1 30); do
  if curl -fsS -o /dev/null http://127.0.0.1:3001/; then echo DASH_OK_3001; break; fi
  sleep 1
done
tail -20 /tmp/gpumesh-dash.log || true
curl -fsS -o /dev/null -w "DASH:%{http_code}\n" http://127.0.0.1:3001/ || echo DASH_SKIP

# alice share
if ! pgrep -f "target/debug/gpumesh share" >/dev/null; then
  HOME=/tmp/gm-alice nohup "$GM" share > /tmp/alice-share.log 2>&1 &
  sleep 2
fi

echo "==== SCHEDULE ===="
timeout 120 env HOME=/tmp/gm-bob "$GM" run --group research --gpu-memory 1GB --workdir /tmp --image python:3.12-slim echo scheduled-ok
echo SCHED_EC:$?

echo "==== SYNC ===="
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
echo DONE
