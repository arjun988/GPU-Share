"use client";

import Link from "next/link";
import { fmtAgo, type LocalPeer } from "@/lib/api";
import { usePoll } from "@/lib/use-poll";
import { Badge, Empty, PageHeader } from "@/components/ui";

export default function PeersPage() {
  const { data: peers, error } = usePoll<LocalPeer[]>("/v1/local/peers");

  return (
    <div>
      <PageHeader
        title="Peers"
        subtitle="Paired nodes on this machine. Manage allowlists on Connect."
      />
      {error ? (
        <Empty>{error}</Empty>
      ) : !peers || peers.length === 0 ? (
        <Empty>
          No peers. <Link href="/connect">Pair a peer</Link>.
        </Empty>
      ) : (
        <div className="panel overflow-x-auto">
          <table className="data-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>GPU</th>
                <th>VRAM</th>
                <th>Access</th>
                <th>Seen</th>
              </tr>
            </thead>
            <tbody>
              {peers.map((p) => (
                <tr key={p.node_id}>
                  <td>
                    <p className="font-medium text-mist-100">{p.node_name}</p>
                    <p className="font-mono text-[11px] text-mist-500">
                      {p.node_id_short}
                    </p>
                  </td>
                  <td className="text-mist-300">{p.gpu_model ?? "—"}</td>
                  <td className="text-mist-300">
                    {p.vram_mb != null
                      ? `${Math.round(p.vram_mb / 1024)} GB`
                      : "—"}
                  </td>
                  <td>
                    <Badge tone={p.allowed ? "ok" : "bad"}>
                      {p.allowed ? "allowed" : "denied"}
                    </Badge>
                  </td>
                  <td className="text-mist-500">{fmtAgo(p.last_seen)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
