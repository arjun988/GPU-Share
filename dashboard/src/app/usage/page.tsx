import { apiGet } from "@/lib/api";
import { Empty, PageHeader, Stat } from "@/components/ui";

export const dynamic = "force-dynamic";

export default async function UsagePage() {
  let usage: {
    jobs_total: number;
    jobs_succeeded: number;
    jobs_failed: number;
    nodes: number;
  } | null = null;
  let error: string | null = null;
  try {
    usage = await apiGet("/v1/usage");
  } catch (e) {
    error = e instanceof Error ? e.message : "error";
  }

  return (
    <div>
      <PageHeader title="Usage" subtitle="Aggregate job counters from synced nodes." />
      {error || !usage ? (
        <Empty>{error ?? "No usage data"}</Empty>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Stat label="Jobs total" value={usage.jobs_total} />
          <Stat label="Succeeded" value={usage.jobs_succeeded} />
          <Stat label="Failed" value={usage.jobs_failed} />
          <Stat label="Nodes" value={usage.nodes} />
        </div>
      )}
    </div>
  );
}
