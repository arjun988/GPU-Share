"use client";

import Link from "next/link";
import { usePoll } from "@/lib/use-poll";
import {
  gpuUtil,
  vramPct,
  type LocalJob,
  type LocalStatus,
} from "@/lib/api";
import { Badge, Empty, PageHeader, Stat, UtilBar } from "@/components/ui";

export default function OverviewPage() {
  const { data: status, error, loading } = usePoll<LocalStatus>(
    "/v1/local/status"
  );
  const { data: jobs } = usePoll<LocalJob[]>("/v1/local/jobs", 4000);

  if (error) {
    return (
      <div>
        <PageHeader
          title="Overview"
          subtitle="Live view of this machine’s GPUMesh node."
        />
        <Empty>
          Cannot reach control plane ({error}). Start{" "}
          <code>cargo run -p gpumesh-control</code> then refresh.
        </Empty>
      </div>
    );
  }

  if (loading && !status) {
    return (
      <div>
        <PageHeader title="Overview" />
        <Empty>Loading…</Empty>
      </div>
    );
  }

  if (status && !status.initialized) {
    return (
      <div>
        <PageHeader title="Overview" />
        <Empty>
          Node not initialized. Run <code>gpumesh init</code> on this host.
        </Empty>
      </div>
    );
  }

  const gpus = status?.gpus ?? [];
  const recent = (jobs ?? []).slice(0, 6);

  return (
    <div>
      <PageHeader
        title="Overview"
        subtitle={`${status?.node_name ?? "node"} · ${status?.node_id_short ?? ""} · port ${status?.listen_port ?? "—"}`}
      />
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Stat
          label="GPUs"
          value={gpus.length}
          hint={
            gpus[0]
              ? `${gpus[0].name} · ${Math.round(gpus[0].vram_total_mb / 1024)} GB`
              : "No NVIDIA GPU detected"
          }
        />
        <Stat
          label="Peers"
          value={status?.peers ?? 0}
          hint="Paired in ~/.gpumesh"
        />
        <Stat
          label="Jobs running"
          value={status?.jobs_running ?? 0}
          hint={`${status?.jobs_total ?? 0} total`}
        />
        <Stat
          label="Share"
          value={status?.share_running ? "on" : "off"}
          hint={
            status?.share_running
              ? `pid ${status.share_pid ?? "?"}`
              : "gpumesh share"
          }
        />
      </div>

      {gpus.length > 0 ? (
        <section className="mt-8">
          <div className="mb-3 flex items-baseline justify-between">
            <h2 className="section-title">GPUs</h2>
            <Link
              href="/gpus"
              className="text-xs font-medium text-accent hover:text-accent-dim"
            >
              Metrics
            </Link>
          </div>
          <div className="space-y-3">
            {gpus.map((g) => (
              <div key={g.index} className="panel p-4">
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                  <p className="font-medium text-mist-100">{g.name}</p>
                  <Badge tone={gpuUtil(g) > 70 ? "warn" : "ok"}>
                    {gpuUtil(g)}% util
                    {g.temperature_c != null ? ` · ${g.temperature_c}°C` : ""}
                  </Badge>
                </div>
                <UtilBar pct={gpuUtil(g)} />
                <p className="mt-3 text-xs text-mist-500">
                  VRAM {g.vram_used_mb} / {g.vram_total_mb} MB ({vramPct(g)}%)
                </p>
                <UtilBar pct={vramPct(g)} />
              </div>
            ))}
          </div>
        </section>
      ) : null}

      <section className="mt-8">
        <div className="mb-3 flex items-baseline justify-between">
          <h2 className="section-title">Recent jobs</h2>
          <Link
            href="/jobs"
            className="text-xs font-medium text-accent hover:text-accent-dim"
          >
            All jobs
          </Link>
        </div>
        {recent.length === 0 ? (
          <Empty>
            No jobs yet. Use <Link href="/jobs">Jobs</Link> or{" "}
            <Link href="/connect">Connect</Link>.
          </Empty>
        ) : (
          <div className="panel overflow-x-auto">
            <table className="data-table">
              <thead>
                <tr>
                  <th>Job</th>
                  <th>Peer</th>
                  <th>State</th>
                </tr>
              </thead>
              <tbody>
                {recent.map((j) => (
                  <tr key={j.job_id}>
                    <td className="font-mono text-mist-100">{j.job_id}</td>
                    <td className="text-mist-300">{j.peer ?? "—"}</td>
                    <td className="text-mist-300">{j.state}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <p className="mt-6 text-xs text-mist-500">
        Updated {status?.updated_at ?? "—"}
      </p>
    </div>
  );
}
