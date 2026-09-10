#!/bin/bash
set -euo pipefail
BIN=/mnt/c/Users/Arjun/Desktop/GPU-Share/target/debug/gpumesh
W=/tmp/gm-smoke2
rm -rf "$W"
mkdir -p "$W/a" "$W/b"
A() { GPUMESH_HOME="$W/a" "$BIN" "$@"; }
B() { GPUMESH_HOME="$W/b" "$BIN" "$@"; }
extract() { awk 'length($0)>=80 {print; exit}'; }
A init --name alice-pc >/dev/null
B init --name bob-laptop >/dev/null
A config set listen_port 47121 >/dev/null
B config set listen_port 47122 >/dev/null
AC=$(A pair-code 2>/dev/null | extract)
BC=$(B pair-code 2>/dev/null | extract)
B pair "$AC" >/dev/null
A pair "$BC" >/dev/null
A group create research >/dev/null
INV=$(A group invite research 2>/dev/null | extract)
B group join "$INV" >/dev/null
A group add research bob-laptop >/dev/null
GPUMESH_HOME="$W/a" "$BIN" share >/tmp/gm-share.out 2>&1 &
SP=$!
sleep 2
echo CONNECT:
B connect alice-pc
echo PEERS_BOB:
B peers
echo STATUS_ALICE:
A status
kill "$SP" 2>/dev/null || true
GPUMESH_HOME="$W/a" "$BIN" share stop >/dev/null 2>&1 || true
echo SMOKE2_OK
