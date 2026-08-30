"use client";

import { useState, type FormEvent } from "react";
import { apiPost, type LocalJob } from "@/lib/api";
import { usePoll } from "@/lib/use-poll";
import { LogPane } from "@/components/log-pane";
import {
  Badge,
  Btn,
  Empty,
  Field,
  PageHeader,
  inputClass,
} from "@/components/ui";

export default function JobsPage() {
  const { data: jobs, error, reload } = usePoll<LocalJob[]>("/v1/local/jobs");
  const [selected, setSelected] = useState<string | null>(null);
  const [peer, setPeer] = useState("");
  const [image, setImage] = useState("");
  const [command, setCommand] = useState("nvidia-smi");
  const [workdir, setWorkdir] = useState("");
  const [gpuMemory, setGpuMemory] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function onRun(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setMsg(null);
    try {
      const parts =
        command
          .trim()
          .match(/(?:[^\s"]+|"[^"]*")+/g)
          ?.map((s) => s.replace(/^"|"$/g, "")) ?? [];
      if (parts.length === 0) throw new Error("command required");
      const job = await apiPost<LocalJob>("/v1/local/run", {
        peer: peer.trim(),
        image: image.trim() || undefined,
        command: parts,
        workdir: workdir.trim() || undefined,
        gpu_memory: gpuMemory.trim() || undefined,
      });
      setSelected(job.job_id);
      setMsg(`Started job ${job.job_id}`);
      await reload();
    } catch (err) {
      setMsg(err instanceof Error ? err.message : "run failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <PageHeader
        title="Jobs"
        subtitle="Local job history, live logs, and remote run from this dashboard."
      />

      {msg ? <p className="notice">{msg}</p> : null}

      <form
        className="panel mb-8 grid gap-4 p-5 md:grid-cols-2"
        onSubmit={onRun}
      >
        <h2 className="section-title md:col-span-2">Run on peer</h2>
        <Field label="Peer">
          <input
            className={inputClass}
            value={peer}
            onChange={(e) => setPeer(e.target.value)}
            placeholder="bob"
            required
          />
        </Field>
        <Field label="Image (optional)">
          <input
            className={inputClass}
            value={image}
            onChange={(e) => setImage(e.target.value)}
            placeholder="default CUDA image"
          />
        </Field>
        <Field label="Command">
          <input
            className={inputClass}
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            required
          />
        </Field>
        <Field label="GPU memory (optional)">
          <input
            className={inputClass}
            value={gpuMemory}
            onChange={(e) => setGpuMemory(e.target.value)}
            placeholder="8GB"
          />
        </Field>
        <Field label="Workdir on this machine (optional)">
          <input
            className={inputClass}
            value={workdir}
            onChange={(e) => setWorkdir(e.target.value)}
            placeholder="/path/to/project"
          />
        </Field>
        <div className="flex items-end">
          <Btn type="submit" kind="primary" disabled={busy}>
            Run
          </Btn>
        </div>
      </form>

      {error ? (
        <Empty>{error}</Empty>
      ) : !jobs || jobs.length === 0 ? (
        <Empty>No jobs yet.</Empty>
      ) : (
        <div className="grid gap-6 lg:grid-cols-2">
          <div className="panel overflow-x-auto">
            <table className="data-table">
              <thead>
                <tr>
                  <th>Job</th>
                  <th>Peer</th>
                  <th>State</th>
                </tr>
              </thead>
              <tbody>
                {jobs.map((j) => (
                  <tr
                    key={j.job_id}
                    className={`cursor-pointer hover:bg-ink-700 ${
                      selected === j.job_id ? "bg-accent-soft" : ""
                    }`}
                    onClick={() => setSelected(j.job_id)}
                  >
                    <td className="font-mono text-mist-100">{j.job_id}</td>
                    <td className="text-mist-300">{j.peer ?? "—"}</td>
                    <td>
                      <Badge
                        tone={
                          j.state === "SUCCEEDED"
                            ? "ok"
                            : j.state === "FAILED"
                              ? "bad"
                              : "warn"
                        }
                      >
                        {j.state}
                      </Badge>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div>
            <h2 className="section-title mb-3">
              {selected ? `Log · ${selected}` : "Job log"}
            </h2>
            {selected && jobs.find((j) => j.job_id === selected)?.command ? (
              <p className="mb-2 font-mono text-[11px] text-mist-500">
                {jobs.find((j) => j.job_id === selected)?.command.join(" ")}
              </p>
            ) : null}
            <LogPane
              path={selected ? `/v1/local/jobs/${selected}/logs` : null}
              empty="Select a job to stream its log."
            />
          </div>
        </div>
      )}
    </div>
  );
}
