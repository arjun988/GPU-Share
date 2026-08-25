import { apiGet, type GpuRow } from "@/lib/api";
import { Empty, PageHeader, UtilBar } from "@/components/ui";

export const dynamic = "force-dynamic";

export default async function GpusPage() {
  let gpus: GpuRow[] = [];
  let error: string | null = null;
  try {
    gpus = await apiGet<GpuRow[]>("/v1/gpus");
  } catch (e) {
    error = e instanceof Error ? e.message : "error";
  }

  return (
    <div>
      <PageHeader
        title="My GPUs"
        subtitle="Synced GPU inventory from provider nodes."
      />
      {error ? (
        <Empty>{error}</Empty>
      ) : gpus.length === 0 ? (
        <Empty>
          No GPUs yet. Run <code className="text-mist-300">gpumesh sync</code>{" "}
          on a machine with NVIDIA GPUs.
        </Empty>
      ) : (
        <div className="space-y-4">
          {gpus.map((g) => {
            const util = g.utilization ?? 0;
            const vramPct =
              g.vram_total_mb > 0
                ? Math.round((g.vram_used_mb / g.vram_total_mb) * 100)
                : 0;
            return (
              <div key={`${g.node_id}-${g.index}`} className="panel p-5">
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                  <h2 className="font-display text-xl text-mist-100">
                    {g.name}
                  </h2>
                  <p className="text-xs text-mist-500">{g.node_name}</p>
                </div>
                <p className="mt-3 text-sm text-mist-300">
                  Utilization {util}%
                </p>
                <UtilBar pct={util} />
                <p className="mt-4 text-sm text-mist-300">
                  VRAM {g.vram_used_mb} / {g.vram_total_mb} MB ({vramPct}%)
                </p>
                <UtilBar pct={vramPct} />
                {g.temperature_c != null ? (
                  <p className="mt-3 text-xs text-mist-500">
                    Temp {g.temperature_c}°C
                  </p>
                ) : null}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
