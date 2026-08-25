import { apiGet } from "@/lib/api";
import { Empty, PageHeader } from "@/components/ui";

export const dynamic = "force-dynamic";

export default async function SettingsPage() {
  let settings: Record<string, string> | null = null;
  let error: string | null = null;
  try {
    settings = await apiGet("/v1/settings");
  } catch (e) {
    error = e instanceof Error ? e.message : "error";
  }

  const api = process.env.NEXT_PUBLIC_GPUMESH_API ?? "http://127.0.0.1:8080";

  return (
    <div>
      <PageHeader
        title="Settings"
        subtitle="Dashboard and API configuration (read-only in Phase 6)."
      />
      {error ? <Empty>{error}</Empty> : null}
      <div className="panel space-y-4 p-6 text-sm">
        <Row k="API endpoint" v={api} />
        <Row k="Product" v={settings?.product ?? "GPUMesh Cloud"} />
        <Row k="Phase" v={settings?.phase ?? "6"} />
        <Row k="API version" v={settings?.api_version ?? "v1"} />
        <p className="pt-2 text-xs text-mist-500">
          Point CLI at this API with{" "}
          <code className="text-mist-300">
            gpumesh config set rendezvous_url {api}
          </code>
        </p>
      </div>
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex flex-wrap justify-between gap-2 border-b border-white/5 pb-3">
      <span className="text-mist-500">{k}</span>
      <span className="text-mist-100">{v}</span>
    </div>
  );
}
