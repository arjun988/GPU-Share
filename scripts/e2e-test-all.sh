#!/usr/bin/env bash
set -eu
export PATH="$HOME/.cargo/bin:$PATH"
ROOT=/mnt/c/Users/Arjun/Desktop/GPU-Share
GO="/mnt/c/Program Files/Go/bin/go.exe"
GM="$ROOT/target/debug/gpumesh"
cd "$ROOT"

echo "======== BUILD CLI ========"
cargo build -p gpumesh-cli
test -x "$GM"
"$GM" --version

echo "======== DOCKER ========"
docker version --format '{{.Server.Version}}' || echo "DOCKER_FAIL"

echo "======== START CONTROL PLANE ========"
pkill -f 'control-plane' 2>/dev/null || true
pkill -f 'go.exe run' 2>/dev/null || true
cd "$ROOT/services/control-plane"
"$GO" run . > /tmp/gpumesh-api.log 2>&1 &
echo $! > /tmp/gpumesh-api.pid
sleep 3
curl -fsS http://127.0.0.1:8080/healthz && echo " API_OK"

echo "======== START DASHBOARD ========"
cd "$ROOT/dashboard"
if [ ! -d node_modules ]; then
  npm install
fi
pkill -f 'next dev' 2>/dev/null || true
npm run dev > /tmp/gpumesh-dash.log 2>&1 &
echo $! > /tmp/gpumesh-dash.pid
sleep 8
curl -fsS -o /dev/null -w "%{http_code}" http://127.0.0.1:3000/ || true
echo

echo "======== TWO-NODE E2E ========"
rm -rf /tmp/gm-alice /tmp/gm-bob
mkdir -p /tmp/gm-alice /tmp/gm-bob
HOME=/tmp/gm-alice "$GM" init --name alice
HOME=/tmp/gm-bob "$GM" init --name bob
# bob different port
printf '%s\n' 'node_name = "bob"' 'listen_port = 47001' 'default_image = "python:3.12-slim"' 'max_concurrent_jobs = 1' 'sharing_enabled = false' 'default_retries = 0' > /tmp/gm-bob/.gpumesh/config.toml

HOME=/tmp/gm-bob "$GM" pair-code > /tmp/bob-code.log 2>&1
HOME=/tmp/gm-alice "$GM" pair-code > /tmp/alice-code.log 2>&1
grep -E '^eyJ' /tmp/bob-code.log > /tmp/bobcode.txt
grep -E '^eyJ' /tmp/alice-code.log > /tmp/alicecode.txt

HOME=/tmp/gm-alice "$GM" pair "$(cat /tmp/bobcode.txt)"
HOME=/tmp/gm-bob "$GM" pair "$(cat /tmp/alicecode.txt)"

HOME=/tmp/gm-alice "$GM" group create research
HOME=/tmp/gm-alice "$GM" group add research bob
HOME=/tmp/gm-alice "$GM" group list
HOME=/tmp/gm-alice "$GM" group members research

# alice share
pkill -f 'gpumesh share' 2>/dev/null || true
HOME=/tmp/gm-alice nohup "$GM" share > /tmp/alice-share.log 2>&1 &
sleep 2
pgrep -a gpumesh || true

echo "======== DOCTOR / STATUS ========"
HOME=/tmp/gm-alice "$GM" doctor || true
HOME=/tmp/gm-bob "$GM" status || true

echo "======== CONNECT + RUN ========"
HOME=/tmp/gm-bob "$GM" connect alice
timeout 120 env HOME=/tmp/gm-bob "$GM" run --peer alice --workdir /tmp --image python:3.12-slim echo hello-phase56
echo RUN_EC:$?

echo "======== SCHEDULE RUN ========"
timeout 90 env HOME=/tmp/gm-bob "$GM" run --group research --gpu-memory 1GB --workdir /tmp --image python:3.12-slim echo scheduled-ok
echo SCHED_EC:$?

echo "======== SYNC + API ========"
HOME=/tmp/gm-alice "$GM" config set rendezvous_url http://127.0.0.1:8080
HOME=/tmp/gm-alice "$GM" sync
curl -fsS http://127.0.0.1:8080/v1/overview
echo
curl -fsS http://127.0.0.1:8080/v1/gpus | head -c 400
echo
curl -fsS http://127.0.0.1:8080/v1/jobs | head -c 400
echo
curl -fsS -o /dev/null -w "DASH_HTTP:%{http_code}\n" http://127.0.0.1:3000/

echo "======== DONE ========"
