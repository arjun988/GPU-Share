"use client";

import { usePoll } from "@/lib/use-poll";
import type { LocalNetwork, LocalStatus } from "@/lib/api";
import { Empty, PageHeader, Stat } from "@/components/ui";

export default function NetworkPage() {
  const { data: net, error } = usePoll<LocalNetwork>("/v1/local/network");
  const { data: status } = usePoll<LocalStatus>("/v1/local/status", 5000);

  return (
    <div>
      <PageHeader
        title="Network"
        subtitle="Listen port, share process, and private groups."
      />
      {error ? (
        <Empty>{error}</Empty>
      ) : (
        <>
          <div className="mb-6 grid gap-3 sm:grid-cols-3">
            <Stat label="Listen port" value={net?.listen_port ?? "—"} />
            <Stat
              label="Share process"
              value={net?.share_running ? "Running" : "Stopped"}
              hint={net?.share_pid ? `pid ${net.share_pid}` : undefined}
            />
            <Stat
              label="Peers / groups"
              value={`${status?.peers ?? 0} / ${net?.groups.length ?? 0}`}
            />
          </div>
          <div className="panel overflow-hidden">
            <div className="border-b border-line px-5 py-3.5">
              <h2 className="section-title">Groups</h2>
            </div>
            {!net || net.groups.length === 0 ? (
              <p className="px-5 py-10 text-center text-sm text-mist-500">
                No groups. Create with{" "}
                <code>gpumesh group create research</code>
              </p>
            ) : (
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Members</th>
                    <th>ID</th>
                  </tr>
                </thead>
                <tbody>
                  {net.groups.map((g) => (
                    <tr key={g.id}>
                      <td className="font-medium text-mist-100">{g.name}</td>
                      <td className="text-mist-300">{g.members}</td>
                      <td className="font-mono text-xs text-mist-500">{g.id}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
          {net?.home ? (
            <p className="mt-4 text-xs text-mist-500">Home {net.home}</p>
          ) : null}
        </>
      )}
    </div>
  );
}
