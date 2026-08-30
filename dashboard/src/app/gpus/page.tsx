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
        subtitle="Live NVML metrics from this machine."
      />
      {error ? (
        <Empty>{error}</Empty>
      ) : gpus.length === 0 ? (
        <Empty>
          No NVIDIA GPU detected. Check drivers / WSL CUDA /{" "}
          <code>gpumesh doctor</code>.
        </Empty>
      ) : (
        <div className="grid gap-4 lg:grid-cols-2">
          {gpus.map((g) => {
            const util = gpuUtil(g);
            const vp = vramPct(g);
            return (
              <div key={g.index} className="panel p-5">
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div>
                    <p className="text-[11px] font-medium uppercase tracking-wide text-mist-500">
                      Device {g.index}
                    </p>
                    <h2 className="mt-1 text-lg font-semibold text-mist-100">
                      {g.name}
                    </h2>
                  </div>
                  <Badge tone={util > 80 ? "warn" : "ok"}>{util}% util</Badge>
                </div>
                <div className="mt-5 space-y-4">
                  <div>
                    <div className="flex justify-between text-sm text-mist-300">
                      <span>Utilization</span>
                      <span className="tabular-nums">{util}%</span>
                    </div>
                    <UtilBar pct={util} />
                  </div>
                  <div>
                    <div className="flex justify-between text-sm text-mist-300">
                      <span>VRAM</span>
                      <span className="tabular-nums">
                        {g.vram_used_mb} / {g.vram_total_mb} MB
                      </span>
                    </div>
                    <UtilBar pct={vp} />
                  </div>
                </div>
                <div className="mt-5 grid grid-cols-2 gap-3 border-t border-line pt-4 text-xs text-mist-500">
                  <Meta
                    label="Temperature"
                    value={
                      g.temperature_c != null ? `${g.temperature_c}°C` : "—"
                    }
                  />
                  <Meta
                    label="Power"
                    value={g.power_watts != null ? `${g.power_watts} W` : "—"}
                  />
                  <Meta label="Driver" value={g.driver_version ?? "—"} />
                  <Meta label="CUDA" value={g.cuda_version ?? "—"} />
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-mist-500">{label}</p>
      <p className="mt-0.5 font-medium text-mist-100">{value}</p>
    </div>
  );
}
