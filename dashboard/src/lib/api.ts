function apiBase(): string {
  const env = process.env.NEXT_PUBLIC_GPUMESH_API;
  if (env) return env.replace(/\/$/, "");
  if (typeof window !== "undefined") return "/gpumesh";
  return process.env.GPUMESH_API_INTERNAL ?? "http://127.0.0.1:8080";
}

function authHeaders(): HeadersInit {
  const token = process.env.NEXT_PUBLIC_GPUMESH_API_TOKEN;
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function parseError(res: Response, path: string): Promise<string> {
  const text = await res.text();
  try {
    const j = JSON.parse(text) as { error?: string };
    if (j.error) return j.error;
  } catch {
    /* not json */
  }
  return text || `API ${path} → ${res.status}`;
}

export async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(`${apiBase()}${path}`, {
    cache: "no-store",
    headers: authHeaders(),
  });
  if (!res.ok) {
    throw new Error(await parseError(res, path));
  }
  return res.json() as Promise<T>;
}

export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${apiBase()}${path}`, {
    method: "POST",
    cache: "no-store",
    headers: {
      "Content-Type": "application/json",
      ...authHeaders(),
    },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new Error(await parseError(res, path));
  }
  const text = await res.text();
  return (text ? JSON.parse(text) : {}) as T;
}

export type GpuRow = {
  index: number;
  name: string;
  uuid?: string;
  vram_total_mb: number;
  vram_used_mb: number;
  vram_free_mb: number;
  utilization_gpu?: number;
  utilization_mem?: number;
  utilization?: number;
  temperature_c?: number;
  power_watts?: number;
  driver_version?: string;
  cuda_version?: string;
  node_id?: string;
  node_name?: string;
};

export type LocalStatus = {
  initialized: boolean;
  node_name: string;
  node_id: string;
  node_id_short: string;
  listen_port: number;
  sharing_enabled: boolean;
  share_pid?: number | null;
  share_running: boolean;
  home: string;
  gpus: GpuRow[];
  peers: number;
  jobs_running: number;
  jobs_total: number;
  groups: number;
  updated_at: string;
};

export type LocalPeer = {
  node_id: string;
  node_id_short: string;
  node_name: string;
  addrs: string[];
  gpu_model?: string;
  vram_mb?: number;
  vram_free_mb?: number;
  utilization?: number;
  last_seen?: number | null;
  paired_at: number;
  allowed: boolean;
  desktop_allowed: boolean;
  gpu_remote_allowed: boolean;
  live_status?: string | null;
  sharing?: boolean | null;
};

export type LocalJob = {
  job_id: string;
  peer?: string;
  state: string;
  exit_code?: number | null;
  image: string;
  command: string[];
  error?: string | null;
  created_at: string;
  finished_at?: string | null;
  attempts: number;
  has_log: boolean;
};

export type LogChunk = {
  offset: number;
  size: number;
  truncated: boolean;
  text: string;
  path: string;
};

export type LocalNetwork = {
  listen_port: number;
  share_running: boolean;
  share_pid?: number | null;
  groups: { id: string; name: string; members: number; owner_node_id: string }[];
  home: string;
};

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

export function gpuUtil(g: GpuRow): number {
  return g.utilization_gpu ?? g.utilization ?? 0;
}

export function vramPct(g: GpuRow): number {
  return g.vram_total_mb > 0
    ? Math.round((g.vram_used_mb / g.vram_total_mb) * 100)
    : 0;
}

export function fmtAgo(ts?: number | null): string {
  if (!ts) return "never";
  const s = Math.max(0, Math.floor(Date.now() / 1000 - ts));
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}
