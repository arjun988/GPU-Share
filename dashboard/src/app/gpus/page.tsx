"use client";

import { usePoll } from "@/lib/use-poll";
import { gpuUtil, vramPct, type GpuRow, type LocalStatus } from "@/lib/api";
import { Badge, Empty, PageHeader, UtilBar } from "@/components/ui";

export default function GpusPage() {
  const { data: status, error } = usePoll<LocalStatus>("/v1/local/status");
  const gpus: GpuRow[] = status?.gpus ?? [];

  return (
    <div>
      <PageHeader
        title="GPUs"
        subtitle="Live NVML metrics from this machine (polled every few seconds)."
      />
      {error ? (
        <Empty>{error}</Empty>
      ) : gpus.length === 0 ? (
        <Empty>
          No NVIDIA GPU detected. Check drivers / WSL CUDA /{" "}
          <code>gpumesh doctor</code>.
        </Empty>
      ) : (
        <div className="space-y-4">
          {gpus.map((g) => {
            const util = gpuUtil(g);
            const vp = vramPct(g);
            return (
              <div key={g.index} className="panel p-5">
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                  <h2 className="text-lg font-semibold text-mist-100">{g.name}</h2>
                  <Badge tone={util > 80 ? "warn" : "ok"}>GPU {g.index}</Badge>
                </div>
                <p className="mt-3 text-sm text-mist-300">Utilization {util}%</p>
                <UtilBar pct={util} />
                <p className="mt-4 text-sm text-mist-300">
                  VRAM {g.vram_used_mb} / {g.vram_total_mb} MB ({vp}%)
                </p>
                <UtilBar pct={vp} />
                <div className="mt-4 flex flex-wrap gap-4 text-xs text-mist-500">
                  {g.temperature_c != null ? (
                    <span>Temp {g.temperature_c}°C</span>
                  ) : null}
                  {g.power_watts != null ? (
                    <span>Power {g.power_watts} W</span>
                  ) : null}
                  {g.driver_version ? (
                    <span>Driver {g.driver_version}</span>
                  ) : null}
                  {g.cuda_version ? <span>CUDA {g.cuda_version}</span> : null}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
