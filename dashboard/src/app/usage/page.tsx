"use client";

import { usePoll } from "@/lib/use-poll";
import type { LocalJob, LocalStatus } from "@/lib/api";
import { Empty, PageHeader, Stat } from "@/components/ui";

/** Kept for nav compatibility — live job counters from local jobs. */
export default function UsagePage() {
  const { data: status, error } = usePoll<LocalStatus>("/v1/local/status");
  const { data: jobs } = usePoll<LocalJob[]>("/v1/local/jobs", 5000);
  const list = jobs ?? [];
  const succeeded = list.filter((j) => j.state === "SUCCEEDED").length;
  const failed = list.filter((j) => j.state === "FAILED").length;

  return (
    <div>
      <PageHeader
        title="Usage"
        subtitle="Job counters from this node’s local history."
      />
      {error ? (
        <Empty>{error}</Empty>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Stat label="Jobs total" value={status?.jobs_total ?? list.length} />
          <Stat label="Running" value={status?.jobs_running ?? 0} />
          <Stat label="Succeeded" value={succeeded} />
          <Stat label="Failed" value={failed} />
        </div>
      )}
    </div>
  );
}
