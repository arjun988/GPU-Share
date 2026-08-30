"use client";

import Link from "next/link";
import { usePoll } from "@/lib/use-poll";
import {
  gpuUtil,
  vramPct,
  type LocalJob,
  type LocalStatus,
} from "@/lib/api";
import {
  Badge,
  CardHeader,
  Empty,
  PageHeader,
  Stat,
  UtilBar,
} from "@/components/ui";

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

      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <Stat
          label="GPUs"
          value={gpus.length}
          hint={
            gpus[0]
              ? `${gpus[0].name} · ${Math.round(gpus[0].vram_total_mb / 1024)} GB`
              : "No NVIDIA GPU detected"
          }
        />
        <Stat label="Peers" value={status?.peers ?? 0} hint="Paired peers" />
        <Stat
          label="Jobs running"
          value={status?.jobs_running ?? 0}
          hint={`${status?.jobs_total ?? 0} total`}
        />
        <Stat
          label="Share"
          value={status?.share_running ? "On" : "Off"}
          hint={
            status?.share_running
              ? `pid ${status.share_pid ?? "?"}`
              : "gpumesh share"
          }
        />
      </div>

      <div className="mt-6 grid gap-4 lg:grid-cols-5">
        <section className="panel p-5 lg:col-span-3">
          <CardHeader
            title="GPU utilization"
            action={
              <Link href="/gpus" className="text-xs font-medium">
                Details
              </Link>
            }
          />
          {gpus.length === 0 ? (
            <p className="py-8 text-center text-sm text-mist-500">
              No NVIDIA GPU detected.
            </p>
          ) : (
            <div className="space-y-4">
              {gpus.map((g) => (
                <div key={g.index}>
                  <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <p className="text-sm font-medium text-mist-100">{g.name}</p>
                    <Badge tone={gpuUtil(g) > 70 ? "warn" : "ok"}>
                      {gpuUtil(g)}%
                      {g.temperature_c != null ? ` · ${g.temperature_c}°C` : ""}
                    </Badge>
                  </div>
                  <UtilBar pct={gpuUtil(g)} />
                  <p className="mt-2 text-xs text-mist-500">
                    VRAM {g.vram_used_mb}/{g.vram_total_mb} MB ({vramPct(g)}%)
                  </p>
                  <UtilBar pct={vramPct(g)} />
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="panel p-5 lg:col-span-2">
          <CardHeader
            title="Node"
            description="Local identity and share state"
          />
          <dl className="space-y-3 text-sm">
            <Row k="Name" v={status?.node_name ?? "—"} />
            <Row k="ID" v={status?.node_id_short ?? "—"} mono />
            <Row k="Port" v={String(status?.listen_port ?? "—")} />
            <Row
              k="Share"
              v={status?.share_running ? "Running" : "Stopped"}
            />
            <Row k="Home" v={status?.home ?? "—"} mono />
          </dl>
        </section>
      </div>

      <section className="panel mt-4 overflow-hidden">
        <div className="flex items-center justify-between border-b border-line px-5 py-3.5">
          <h2 className="section-title">Recent jobs</h2>
          <Link href="/jobs" className="text-xs font-medium">
            View all
          </Link>
        </div>
        {recent.length === 0 ? (
          <p className="px-5 py-10 text-center text-sm text-mist-500">
            No jobs yet. Use <Link href="/jobs">Jobs</Link> or{" "}
            <Link href="/connect">Connect</Link>.
          </p>
        ) : (
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
        )}
      </section>

      <p className="mt-4 text-xs text-mist-500">
        Updated {status?.updated_at ?? "—"}
      </p>
    </div>
  );
}

function Row({
  k,
  v,
  mono,
}: {
  k: string;
  v: string;
  mono?: boolean;
}) {
  return (
    <div className="flex justify-between gap-3 border-b border-line pb-3 last:border-0 last:pb-0">
      <dt className="text-mist-500">{k}</dt>
      <dd
        className={`truncate text-right text-mist-100 ${mono ? "font-mono text-xs" : "font-medium"}`}
      >
        {v}
      </dd>
    </div>
  );
}
