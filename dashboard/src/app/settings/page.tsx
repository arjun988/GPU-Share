"use client";

import { usePoll } from "@/lib/use-poll";
import type { LocalStatus } from "@/lib/api";
import { useTheme } from "@/lib/theme";
import { Btn, Empty, PageHeader } from "@/components/ui";

export default function SettingsPage() {
  const { data: status, error } = usePoll<LocalStatus>("/v1/local/status", 8000);
  const { data: settings } = usePoll<Record<string, string | boolean>>(
    "/v1/settings",
    15000
  );
  const { mode, resolved, setMode } = useTheme();

  return (
    <div>
      <PageHeader
        title="Settings"
        subtitle="Node identity, appearance, and control plane."
      />
      {error ? <Empty>{error}</Empty> : null}

      <div className="grid gap-4 lg:grid-cols-2">
        <section className="panel overflow-hidden">
          <div className="border-b border-line px-5 py-3.5">
            <h2 className="section-title">Appearance</h2>
          </div>
          <div className="space-y-3 p-5">
            <p className="text-sm text-mist-500">
              Theme preference is stored in this browser. Current:{" "}
              <span className="font-medium text-mist-100">
                {mode}
                {mode === "system" ? ` (${resolved})` : ""}
              </span>
            </p>
            <div className="flex flex-wrap gap-2">
              <Btn
                kind={mode === "light" ? "primary" : "ghost"}
                onClick={() => setMode("light")}
              >
                Light
              </Btn>
              <Btn
                kind={mode === "dark" ? "primary" : "ghost"}
                onClick={() => setMode("dark")}
              >
                Dark
              </Btn>
              <Btn
                kind={mode === "system" ? "primary" : "ghost"}
                onClick={() => setMode("system")}
              >
                System
              </Btn>
            </div>
          </div>
        </section>

        <section className="panel overflow-hidden">
          <div className="border-b border-line px-5 py-3.5">
            <h2 className="section-title">Node</h2>
          </div>
          <div className="divide-y divide-line text-sm">
            <Row k="Name" v={status?.node_name ?? "—"} />
            <Row k="Node ID" v={status?.node_id_short ?? "—"} />
            <Row k="Listen port" v={String(status?.listen_port ?? "—")} />
            <Row k="Home" v={status?.home ?? "—"} />
            <Row
              k="Sharing"
              v={
                status?.share_running
                  ? `running (pid ${status.share_pid ?? "?"})`
                  : status?.sharing_enabled
                    ? "enabled (process not detected)"
                    : "off"
              }
            />
            <Row k="Product" v={String(settings?.product ?? "GPUMesh")} />
          </div>
          <p className="border-t border-line px-5 py-4 text-xs text-mist-500">
            Optional token: set <code>GPUMESH_API_TOKEN</code> on the control
            plane and <code>NEXT_PUBLIC_GPUMESH_API_TOKEN</code> for the UI.
          </p>
        </section>
      </div>
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex flex-wrap justify-between gap-2 px-5 py-3.5">
      <span className="text-mist-500">{k}</span>
      <span className="max-w-[70%] break-all text-right font-medium text-mist-100">
        {v}
      </span>
    </div>
  );
}
