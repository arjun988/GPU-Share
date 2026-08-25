const API = process.env.NEXT_PUBLIC_GPUMESH_API ?? "http://127.0.0.1:8080";

export async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(`${API}${path}`, {
    cache: "no-store",
    next: { revalidate: 0 },
  });
  if (!res.ok) {
    throw new Error(`API ${path} → ${res.status}`);
  }
  return res.json() as Promise<T>;
}

export type Overview = {
  gpus_online: number;
  gpus_available: number;
  running_jobs: number;
  total_vram_gb: number;
  peers: number;
  groups: number;
  nodes: number;
  updated_at: string;
};

export type GpuRow = {
  index: number;
  name: string;
  vram_total_mb: number;
  vram_used_mb: number;
  vram_free_mb: number;
  utilization?: number;
  temperature_c?: number;
  node_id: string;
  node_name: string;
};

export type PeerRow = {
  node_id: string;
  node_name: string;
  gpu_model?: string;
  vram_mb?: number;
  vram_free_mb?: number;
  utilization?: number;
  sharing: boolean;
};

export type JobRow = {
  job_id: string;
  peer?: string;
  state: string;
  exit_code?: number;
  image: string;
  created_at: string;
  node_id: string;
};

export type GroupRow = {
  id: string;
  name: string;
  members: number;
  owner_node_id: string;
};
