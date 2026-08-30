"use client";

import { usePoll } from "@/lib/use-poll";
import type { LocalStatus } from "@/lib/api";
import { Empty, PageHeader } from "@/components/ui";

export default function SettingsPage() {
  const { data: status, error } = usePoll<LocalStatus>("/v1/local/status", 8000);
  const { data: settings } = usePoll<Record<string, string | boolean>>(
    "/v1/settings",
    15000
  );

  return (
    <div>
      <PageHeader
        title="Settings"
        subtitle="Read-only view of this node and the control plane."
      />
      {error ? <Empty>{error}</Empty> : null}
      <div className="panel space-y-0 p-0 text-sm">
        <Row k="Node" v={status?.node_name ?? "—"} />
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
        <Row k="Local console" v={settings?.local_console ? "yes" : "no"} last />
        <p className="px-6 py-4 text-xs text-mist-500">
          Optional token: set <code>GPUMESH_API_TOKEN</code> on the control plane
          and <code>NEXT_PUBLIC_GPUMESH_API_TOKEN</code> for the UI.
        </p>
      </div>
    </div>
  );
}

function Row({
  k,
  v,
  last,
}: {
  k: string;
  v: string;
  last?: boolean;
}) {
  return (
    <div
      className={`flex flex-wrap justify-between gap-2 px-6 py-3.5 ${
        last ? "" : "border-b border-line"
      }`}
    >
      <span className="text-mist-500">{k}</span>
      <span className="max-w-[70%] break-all text-right font-medium text-mist-100">
        {v}
      </span>
    </div>
  );
}
