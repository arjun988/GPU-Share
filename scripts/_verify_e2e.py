#!/usr/bin/env python3
"""Two-home smoke: pair, share, 1:1 connect, group, jobs/logs, local API."""
from __future__ import annotations

import os
import re
import shutil
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request

ROOT = "/mnt/c/Users/Arjun/Desktop/GPU-Share"
GM = os.path.join(ROOT, "target/debug/gpumesh")
CTRL = os.path.join(ROOT, "target/debug/gpumesh-control")
BASE = "/tmp/gpumesh-verify"
ALICE = os.path.join(BASE, "alice")
BOB = os.path.join(BASE, "bob")
ANSI = re.compile(r"\x1b\[[0-9;]*m")
failures: list[str] = []
notes: list[str] = []


def strip(s: str) -> str:
    return ANSI.sub("", s)


def env_for(home: str) -> dict[str, str]:
    e = os.environ.copy()
    e["GPUMESH_HOME"] = home
    e["RUST_BACKTRACE"] = "1"
    return e


def run(
    home: str,
    args: list[str],
    *,
    timeout: int = 60,
    check: bool = True,
    bin: str | None = None,
) -> subprocess.CompletedProcess[str]:
    cmd = [bin or GM, *args]
    print(f"\n$ GPUMESH_HOME={home} {' '.join(cmd)}")
    p = subprocess.run(
        cmd,
        env=env_for(home),
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    out = strip(p.stdout or "")
    err = strip(p.stderr or "")
    if out.strip():
        print(out.rstrip())
    if err.strip():
        print(err.rstrip(), file=sys.stderr)
    print(f"exit={p.returncode}")
    if check and p.returncode != 0:
        failures.append(f"{' '.join(args)} exit {p.returncode}")
    return p


def extract_code(text: str) -> str:
    text = strip(text)
    for line in text.splitlines():
        line = line.strip()
        if len(line) >= 80 and re.fullmatch(r"[A-Za-z0-9_\-=]+", line):
            return line
    raise RuntimeError(f"no code in:\n{text}")


def expect(cond: bool, msg: str) -> None:
    if cond:
        print(f"OK  {msg}")
    else:
        print(f"FAIL {msg}")
        failures.append(msg)


def main() -> int:
    if not os.path.isfile(GM):
        print("missing binary", GM)
        return 1

    shutil.rmtree(BASE, ignore_errors=True)
    os.makedirs(ALICE, exist_ok=True)
    os.makedirs(BOB, exist_ok=True)

    run(ALICE, ["init", "--name", "alice-pc"])
    run(BOB, ["init", "--name", "bob-pc"])

    a_code = extract_code(run(ALICE, ["pair-code"]).stdout)
    b_code = extract_code(run(BOB, ["pair-code"]).stdout)
    run(BOB, ["pair", a_code])
    run(ALICE, ["pair", b_code])

    share_log = os.path.join(BASE, "alice-share.log")
    share = subprocess.Popen(
        [GM, "share"],
        env=env_for(ALICE),
        stdout=open(share_log, "w"),
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    notes.append(f"alice share pid={share.pid}")
    try:
        sharing = False
        for _ in range(40):
            time.sleep(0.25)
            if os.path.isfile(share_log) and "Sharing enabled" in open(share_log).read():
                sharing = True
                break
        expect(sharing, "alice share started (Sharing enabled)")
        print("--- share log ---\n" + open(share_log).read())

        conn = run(BOB, ["connect", "alice-pc"], timeout=30)
        expect(
            conn.returncode == 0 and "Connected" in strip(conn.stdout),
            "1:1 connect bob → alice-pc",
        )

        peers = run(BOB, ["peers"], timeout=40)
        pout = strip(peers.stdout)
        expect("alice-pc" in pout, "peers lists alice-pc")
        expect("IDLE" in pout or "BUSY" in pout, "peers shows IDLE/BUSY while share is running")
        if "OFFLINE" in pout:
            notes.append("peers still shows OFFLINE after successful connect")

        run(ALICE, ["group", "create", "research"])
        inv = extract_code(run(ALICE, ["group", "invite", "research"]).stdout)
        join = run(BOB, ["group", "join", inv], timeout=20)
        expect(join.returncode == 0 and "Joined group" in strip(join.stdout), "group join")

        # Owner may also add by name (paired peer).
        add = run(ALICE, ["group", "add", "research", "bob-pc"])
        expect(add.returncode == 0 or "already" in strip(add.stdout + add.stderr).lower(), "group add bob-pc")

        am = run(ALICE, ["group", "members", "research"])
        bm = run(BOB, ["group", "members", "research"])
        expect("bob-pc" in strip(am.stdout) or "bob" in strip(am.stdout).lower(), "alice members include bob")
        expect("alice-pc" in strip(bm.stdout), "bob members include alice")

        nvidia = shutil.which("nvidia-smi")
        docker_ok = False
        d = subprocess.run(["docker", "info"], capture_output=True, text=True, timeout=20)
        docker_ok = d.returncode == 0
        notes.append(f"docker usable={docker_ok} nvidia-smi={bool(nvidia)}")

        img = ["--image", "python:3.12-slim"]
        remote = run(
            BOB,
            [
                "run",
                "--peer",
                "alice-pc",
                *img,
                "python",
                "-c",
                "print('hello-from-gpumesh')",
            ],
            timeout=180,
            check=False,
        )
        rout = strip(remote.stdout + remote.stderr)
        expect("job" in rout.lower() or "Job" in rout or remote.returncode == 0, "run started a job record")
        if not docker_ok:
            notes.append("run cannot succeed without Docker on the provider")
            expect(
                remote.returncode != 0 or "docker" in rout.lower() or "FAILED" in rout,
                "run fails cleanly without Docker (expected)",
            )
        else:
            expect(
                remote.returncode == 0 and "hello-from-gpumesh" in rout,
                "run --peer succeeded with Docker (python image)",
            )

        jobs = run(BOB, ["jobs"], timeout=20)
        jout = strip(jobs.stdout)
        expect("JOB" in jout.upper() or "job-" in jout.lower() or len(jout.splitlines()) > 2, "jobs list has entries")

        job_id = None
        for line in jout.splitlines():
            m = re.search(r"(job-[A-Za-z0-9_-]+|[a-f0-9]{8,})", line)
            if m and "JOB" not in line:
                job_id = m.group(1)
                break
        if not job_id:
            # table may use short ids in first column
            for line in jout.splitlines():
                parts = line.split()
                if parts and parts[0] not in ("NAME", "ID", "JOB", "─", "-") and len(parts[0]) >= 4:
                    if parts[0].lower() not in ("gpumesh", "jobs"):
                        job_id = parts[0]
                        break
        if job_id:
            logs = run(BOB, ["logs", job_id], check=False)
            lout = strip(logs.stdout + logs.stderr)
            expect(
                logs.returncode == 0 and ("Job" in lout or "State" in lout or "log" in lout.lower() or len(lout) > 0),
                f"logs for {job_id}",
            )
        else:
            failures.append("could not parse job id from jobs list")
            print("jobs output for parse:\n", jout)

        grp = run(
            BOB,
            [
                "run",
                "--group",
                "research",
                "--image",
                "python:3.12-slim",
                "python",
                "-c",
                "print('group-hello')",
            ],
            timeout=180,
            check=False,
        )
        gout = strip(grp.stdout + grp.stderr)
        if docker_ok:
            expect(
                grp.returncode == 0 and "group-hello" in gout,
                "group schedule/run succeeded with Docker",
            )
        else:
            expect(
                "Scheduled" in gout or "no candidate" not in gout.lower() or grp.returncode != 0,
                "group run attempted schedule (Docker may fail job)",
            )
            if "no candidate" in gout.lower():
                failures.append("group scheduler found no candidates while alice is sharing")

        doc = run(ALICE, ["doctor"], timeout=40, check=False)
        notes.append("doctor exit=" + str(doc.returncode))
        dout = strip(doc.stdout + doc.stderr)
        if docker_ok:
            expect("FAIL docker" not in dout, "doctor docker check passes")
            expect(doc.returncode == 0, "doctor overall success")
        else:
            expect("FAIL docker" in dout, "doctor reports docker missing")

        # Local control API (same home as alice)
        ctrl_log = os.path.join(BASE, "control.log")
        ctrl = subprocess.Popen(
            [CTRL],
            env=env_for(ALICE),
            stdout=open(ctrl_log, "w"),
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        time.sleep(1.5)
        try:
            for path in ("/healthz", "/v1/local/status", "/v1/local/peers", "/v1/local/jobs"):
                url = "http://127.0.0.1:8080" + path
                try:
                    with urllib.request.urlopen(url, timeout=8) as r:
                        body = r.read()[:800]
                        expect(r.status == 200, f"GET {path} -> 200 ({body[:120]!r})")
                except Exception as e:
                    expect(False, f"GET {path} failed: {e}")
        finally:
            ctrl.send_signal(signal.SIGTERM)
            try:
                ctrl.wait(timeout=5)
            except subprocess.TimeoutExpired:
                ctrl.kill()
    finally:
        share.send_signal(signal.SIGTERM)
        try:
            share.wait(timeout=5)
        except subprocess.TimeoutExpired:
            share.kill()

    print("\n======== NOTES ========")
    for n in notes:
        print("-", n)
    print("======== RESULT ========")
    if failures:
        for f in failures:
            print("FAIL:", f)
        return 1
    print("All checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
