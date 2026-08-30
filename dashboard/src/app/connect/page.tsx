"use client";

import { useState, type FormEvent } from "react";
import { apiGet, apiPost, fmtAgo, type LocalPeer } from "@/lib/api";
import { usePoll } from "@/lib/use-poll";
import {
  Badge,
  Btn,
  CardHeader,
  Empty,
  Field,
  PageHeader,
  inputClass,
} from "@/components/ui";

export default function ConnectPage() {
  const { data: peersPoll, error, reload } = usePoll<LocalPeer[]>(
    "/v1/local/peers",
    4000
  );
  const [probed, setProbed] = useState<LocalPeer[] | null>(null);
  const peers = probed ?? peersPoll;
  const [code, setCode] = useState("");
  const [myCode, setMyCode] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function showPairCode() {
    setBusy(true);
    setMsg(null);
    try {
      const r = await apiGet<{ code: string }>("/v1/local/pair-code");
      setMyCode(r.code);
    } catch (e) {
      setMsg(e instanceof Error ? e.message : "failed");
    } finally {
      setBusy(false);
    }
  }

  async function onPair(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setMsg(null);
    try {
      const p = await apiPost<LocalPeer>("/v1/local/pair", { code });
      setMsg(`Paired with ${p.node_name}`);
      setCode("");
      setProbed(null);
      await reload();
    } catch (err) {
      setMsg(err instanceof Error ? err.message : "pair failed");
    } finally {
      setBusy(false);
    }
  }

  async function act(path: string, peer: string, label: string) {
    setBusy(true);
    setMsg(null);
    try {
      const r = await apiPost<Record<string, unknown>>(path, { peer });
      if (path === "/v1/local/connect") {
        setMsg(
          `Connected to ${String(r.peer_name ?? peer)} @ ${String(r.remote_addr)} (${String(r.mode)})`
        );
      } else {
        setMsg(`${label}: ${peer}`);
      }
      setProbed(null);
      await reload();
    } catch (err) {
      setMsg(err instanceof Error ? err.message : "failed");
    } finally {
      setBusy(false);
    }
  }

  async function probe() {
    setBusy(true);
    setMsg(null);
    try {
      const list = await apiGet<LocalPeer[]>("/v1/local/peers?probe=1");
      setProbed(list);
      setMsg("Live probe finished");
    } catch (err) {
      setMsg(err instanceof Error ? err.message : "probe failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <PageHeader
        title="Connect"
        subtitle="Pair peers and manage job, desktop, and CUDA access."
        actions={
          <Btn disabled={busy} onClick={() => void probe()}>
            Probe live
          </Btn>
        }
      />

      {error ? (
        <div className="mb-4">
          <Empty>{error}</Empty>
        </div>
      ) : null}
      {msg ? <p className="notice">{msg}</p> : null}

      <div className="mb-6 grid gap-4 lg:grid-cols-2">
        <div className="panel p-5">
          <CardHeader
            title="Your pair code"
            description="Share out-of-band for mutual pairing"
          />
          <Btn kind="primary" disabled={busy} onClick={() => void showPairCode()}>
            Generate code
          </Btn>
          {myCode ? <pre className="code-block mt-4">{myCode}</pre> : null}
        </div>

        <form className="panel p-5" onSubmit={onPair}>
          <CardHeader
            title="Pair with code"
            description="Paste a peer’s pairing code"
          />
          <Field label="Peer code">
            <textarea
              className={inputClass + " min-h-[5.5rem] font-mono text-xs"}
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="Paste pairing code"
              required
            />
          </Field>
          <div className="mt-4">
            <Btn type="submit" kind="primary" disabled={busy || !code.trim()}>
              Pair
            </Btn>
          </div>
        </form>
      </div>

      <h2 className="section-title mb-3">Peers</h2>
      {!peers || peers.length === 0 ? (
        <Empty>No paired peers yet.</Empty>
      ) : (
        <div className="space-y-3">
          {peers.map((p) => (
            <div key={p.node_id} className="panel p-5">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <p className="text-base font-semibold text-mist-100">
                    {p.node_name}
                  </p>
                  <p className="mt-1 font-mono text-[11px] text-mist-500">
                    {p.node_id_short} · seen {fmtAgo(p.last_seen)}
                  </p>
                </div>
                <div className="flex flex-wrap gap-1.5">
                  <Badge tone={p.allowed ? "ok" : "bad"}>
                    jobs {p.allowed ? "allowed" : "denied"}
                  </Badge>
                  <Badge tone={p.desktop_allowed ? "ok" : "mist"}>
                    desktop {p.desktop_allowed ? "yes" : "no"}
                  </Badge>
                  <Badge tone={p.gpu_remote_allowed ? "ok" : "mist"}>
                    cuda {p.gpu_remote_allowed ? "yes" : "no"}
                  </Badge>
                  {p.live_status ? (
                    <Badge tone={p.live_status === "OFFLINE" ? "bad" : "ok"}>
                      {p.live_status}
                    </Badge>
                  ) : null}
                </div>
              </div>
              <p className="mt-3 text-sm text-mist-300">
                {p.gpu_model ?? "GPU unknown"}
                {p.vram_mb != null
                  ? ` · ${Math.round(p.vram_mb / 1024)} GB`
                  : ""}
              </p>
              {p.addrs.length ? (
                <p className="mt-1 font-mono text-[11px] text-mist-500">
                  {p.addrs.join(" · ")}
                </p>
              ) : null}
              <div className="mt-4 flex flex-wrap gap-2 border-t border-line pt-4">
                <Btn
                  kind="primary"
                  disabled={busy}
                  onClick={() =>
                    void act("/v1/local/connect", p.node_name, "Connect")
                  }
                >
                  Connect
                </Btn>
                <Btn
                  disabled={busy}
                  onClick={() => void act("/v1/local/allow", p.node_name, "Allow")}
                >
                  Allow jobs
                </Btn>
                <Btn
                  disabled={busy}
                  onClick={() =>
                    void act("/v1/local/allow-desktop", p.node_name, "Desktop")
                  }
                >
                  Allow desktop
                </Btn>
                <Btn
                  disabled={busy}
                  onClick={() =>
                    void act("/v1/local/allow-cuda", p.node_name, "CUDA")
                  }
                >
                  Allow CUDA
                </Btn>
                <Btn
                  kind="danger"
                  disabled={busy}
                  onClick={() => void act("/v1/local/deny", p.node_name, "Deny")}
                >
                  Deny
                </Btn>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
