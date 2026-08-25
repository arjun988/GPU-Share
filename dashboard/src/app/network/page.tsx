import { apiGet, type GroupRow } from "@/lib/api";
import { Empty, PageHeader, Stat } from "@/components/ui";

export const dynamic = "force-dynamic";

export default async function NetworkPage() {
  let net: Record<string, unknown> | null = null;
  let groups: GroupRow[] = [];
  let error: string | null = null;
  try {
    net = await apiGet("/v1/network");
    groups = await apiGet("/v1/groups");
  } catch (e) {
    error = e instanceof Error ? e.message : "error";
  }

  return (
    <div>
      <PageHeader
        title="Network"
        subtitle="Control-plane topology. Workload traffic stays peer-to-peer."
      />
      {error ? (
        <Empty>{error}</Empty>
      ) : (
        <>
          <div className="mb-6 grid gap-4 sm:grid-cols-3">
            <Stat label="Nodes" value={String(net?.nodes ?? 0)} />
            <Stat label="Peers" value={String(net?.peers ?? 0)} />
            <Stat label="Groups" value={String(net?.groups ?? 0)} />
          </div>
          <h2 className="mb-3 font-display text-xl text-mist-100">Groups</h2>
          {groups.length === 0 ? (
            <Empty>
              No groups synced. Create with{" "}
              <code className="text-mist-300">gpumesh group create research</code>
            </Empty>
          ) : (
            <div className="panel overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead className="border-b border-white/10 text-xs uppercase text-mist-500">
                  <tr>
                    <th className="px-4 py-3">Name</th>
                    <th className="px-4 py-3">Members</th>
                    <th className="px-4 py-3">ID</th>
                  </tr>
                </thead>
                <tbody>
                  {groups.map((g) => (
                    <tr key={g.id} className="border-b border-white/5">
                      <td className="px-4 py-3 text-mist-100">{g.name}</td>
                      <td className="px-4 py-3 text-mist-300">{g.members}</td>
                      <td className="px-4 py-3 font-mono text-xs text-mist-500">
                        {g.id}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </div>
  );
}
