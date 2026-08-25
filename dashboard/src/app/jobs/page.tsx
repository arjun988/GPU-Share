import { apiGet, type JobRow } from "@/lib/api";
import { Empty, PageHeader } from "@/components/ui";

export const dynamic = "force-dynamic";

export default async function JobsPage() {
  let jobs: JobRow[] = [];
  let error: string | null = null;
  try {
    jobs = await apiGet<JobRow[]>("/v1/jobs");
  } catch (e) {
    error = e instanceof Error ? e.message : "error";
  }

  return (
    <div>
      <PageHeader title="Jobs" subtitle="Recent jobs reported by synced nodes." />
      {error ? (
        <Empty>{error}</Empty>
      ) : jobs.length === 0 ? (
        <Empty>No jobs yet.</Empty>
      ) : (
        <div className="panel overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead className="border-b border-white/10 text-xs uppercase text-mist-500">
              <tr>
                <th className="px-4 py-3">Job</th>
                <th className="px-4 py-3">Peer</th>
                <th className="px-4 py-3">Status</th>
                <th className="px-4 py-3">Image</th>
              </tr>
            </thead>
            <tbody>
              {jobs.map((j) => (
                <tr key={`${j.node_id}-${j.job_id}`} className="border-b border-white/5">
                  <td className="px-4 py-3 font-mono text-mist-100">{j.job_id}</td>
                  <td className="px-4 py-3 text-mist-300">{j.peer ?? "—"}</td>
                  <td className="px-4 py-3 text-mist-300">{j.state}</td>
                  <td className="px-4 py-3 text-mist-500">{j.image}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
