#!/usr/bin/env bash
set -euo pipefail

ROOT=/mnt/c/Users/Arjun/Desktop/GPU-Share
GM="$ROOT/target/debug/gpumesh"
ALICE_HOME=/tmp/gpumesh-alice
BOB_HOME=/tmp/gpumesh-bob
LOG=/tmp/gpumesh-alice.log
PIDFILE=/tmp/gpumesh-alice.pid

cleanup() {
  if [[ -f "$PIDFILE" ]]; then
    kill "$(cat "$PIDFILE")" 2>/dev/null || true
  fi
}
trap cleanup EXIT

rm -rf "$ALICE_HOME" "$BOB_HOME" "$LOG" "$PIDFILE"
mkdir -p "$ALICE_HOME" "$BOB_HOME"

echo "=== INIT alice ==="
HOME="$ALICE_HOME" "$GM" init --name alice
echo "=== INIT bob ==="
HOME="$BOB_HOME" "$GM" init --name bob

echo "=== START alice share ==="
HOME="$ALICE_HOME" "$GM" share >"$LOG" 2>&1 &
echo $! >"$PIDFILE"
sleep 3

echo "=== alice log ==="
cat "$LOG"

CODE=$(awk '/^Pairing code:/{getline; getline; print; exit}' "$LOG" | tr -d '\r')
if [[ -z "$CODE" ]]; then
  # fallback: first long base64-ish line after "Pairing code"
  CODE=$(grep -E '^[A-Za-z0-9_-]{40,}$' "$LOG" | head -1 | tr -d '\r')
fi
echo "=== PAIRING CODE LEN: ${#CODE} ==="
if [[ -z "$CODE" ]]; then
  echo "FAILED: no pairing code"
  exit 1
fi

echo "=== bob pair ==="
HOME="$BOB_HOME" "$GM" pair "$CODE"

echo "=== bob peers ==="
HOME="$BOB_HOME" "$GM" peers || true

echo "=== bob connect alice ==="
HOME="$BOB_HOME" "$GM" connect alice

echo "=== bob run (may fail without Docker) ==="
set +e
HOME="$BOB_HOME" "$GM" run --peer alice --image python:3.12-slim python -c 'print("hello-from-bob")'
RUN_EC=$?
set -e
echo "run exit code: $RUN_EC"

echo "=== DONE ==="
