"use client";

import { useState } from "react";
import { usePoll } from "@/lib/use-poll";
import type { LocalJob } from "@/lib/api";
import { LogPane } from "@/components/log-pane";
import { Btn, Empty, PageHeader, inputClass } from "@/components/ui";

export default function LogsPage() {
  const { data: jobs } = usePoll<LocalJob[]>("/v1/local/jobs", 5000);
  const [tab, setTab] = useState<"agent" | "job">("agent");
  const [jobId, setJobId] = useState<string>("");
  const jobOptions = jobs ?? [];

  return (
    <div>
      <PageHeader
        title="Logs"
        subtitle="Agent share log and per-job stdout/stderr under ~/.gpumesh."
      />

      <div className="panel overflow-hidden">
        <div className="flex flex-wrap items-center gap-2 border-b border-line px-4 py-3">
          <Btn
            kind={tab === "agent" ? "primary" : "ghost"}
            onClick={() => setTab("agent")}
          >
            Agent
          </Btn>
          <Btn
            kind={tab === "job" ? "primary" : "ghost"}
            onClick={() => setTab("job")}
          >
            Job
          </Btn>
          {tab === "job" ? (
            <select
              className={inputClass + " ml-auto w-auto min-w-[12rem]"}
              value={jobId}
              onChange={(e) => setJobId(e.target.value)}
            >
              <option value="">Select job…</option>
              {jobOptions.map((j) => (
                <option key={j.job_id} value={j.job_id}>
                  {j.job_id} · {j.state}
                </option>
              ))}
            </select>
          ) : null}
        </div>
        <div className="p-4">
          {tab === "agent" ? (
            <LogPane
              path="/v1/local/logs/agent"
              empty="No agent.log yet — start sharing with gpumesh share."
            />
          ) : jobId ? (
            <LogPane path={`/v1/local/jobs/${jobId}/logs`} />
          ) : (
            <Empty>Pick a job to follow its log.</Empty>
          )}
        </div>
      </div>
    </div>
  );
}
