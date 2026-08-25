import { apiGet, type PeerRow } from "@/lib/api";
import { Empty, PageHeader } from "@/components/ui";

export const dynamic = "force-dynamic";

export default async function PeersPage() {
  let peers: PeerRow[] = [];
  let error: string | null = null;
  try {
    peers = await apiGet<PeerRow[]>("/v1/peers");
  } catch (e) {
    error = e instanceof Error ? e.message : "error";
  }

  return (
    <div>
      <PageHeader title="Peers" subtitle="Nodes known to the control plane." />
      {error ? (
        <Empty>{error}</Empty>
      ) : peers.length === 0 ? (
        <Empty>No peers synced yet.</Empty>
      ) : (
        <div className="panel overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead className="border-b border-white/10 text-xs uppercase text-mist-500">
              <tr>
                <th className="px-4 py-3">Name</th>
                <th className="px-4 py-3">GPU</th>
                <th className="px-4 py-3">VRAM</th>
                <th className="px-4 py-3">Sharing</th>
              </tr>
            </thead>
            <tbody>
              {peers.map((p) => (
                <tr key={p.node_id} className="border-b border-white/5">
                  <td className="px-4 py-3 text-mist-100">{p.node_name}</td>
                  <td className="px-4 py-3 text-mist-300">
                    {p.gpu_model ?? "—"}
                  </td>
                  <td className="px-4 py-3 text-mist-300">
                    {p.vram_mb != null ? `${Math.round(p.vram_mb / 1024)} GB` : "—"}
                  </td>
                  <td className="px-4 py-3">
                    <span
                      className={
                        p.sharing ? "text-accent" : "text-mist-500"
                      }
                    >
                      {p.sharing ? "yes" : "no"}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
