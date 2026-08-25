import { apiGet, type Overview } from "@/lib/api";
import { Empty, PageHeader, Stat } from "@/components/ui";

export const dynamic = "force-dynamic";

export default async function OverviewPage() {
  let overview: Overview | null = null;
  let error: string | null = null;
  try {
    overview = await apiGet<Overview>("/v1/overview");
  } catch (e) {
    error = e instanceof Error ? e.message : "API unreachable";
  }

  return (
    <div>
      <PageHeader
        title="Overview"
        subtitle="Live view of your private GPUMesh network. Workloads never run on this control plane."
      />
      {error ? (
        <Empty>
          Cannot reach control plane ({error}). Start it with{" "}
          <code className="text-mist-300">go run .</code> in{" "}
          <code className="text-mist-300">services/control-plane</code>, then{" "}
          <code className="text-mist-300">gpumesh sync</code>.
        </Empty>
      ) : overview ? (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Stat label="GPUs online" value={overview.gpus_online} />
          <Stat label="GPUs available" value={overview.gpus_available} />
          <Stat label="Running jobs" value={overview.running_jobs} />
          <Stat
            label="Total VRAM"
            value={`${overview.total_vram_gb} GB`}
            hint={`${overview.nodes} nodes · ${overview.peers} peers · ${overview.groups} groups`}
          />
        </div>
      ) : null}
      {overview ? (
        <p className="mt-6 text-xs text-mist-500">
          Updated {overview.updated_at}
        </p>
      ) : null}
    </div>
  );
}
