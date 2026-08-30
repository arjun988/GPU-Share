"use client";

import { useCallback, useEffect, useState } from "react";
import { apiGet } from "./api";

export function usePoll<T>(path: string | null, intervalMs = 3000) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const reload = useCallback(async () => {
    if (!path) return;
    try {
      const next = await apiGet<T>(path);
      setData(next);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "error");
    } finally {
      setLoading(false);
    }
  }, [path]);

  useEffect(() => {
    setLoading(true);
    void reload();
    if (!path) return;
    const t = setInterval(() => void reload(), intervalMs);
    return () => clearInterval(t);
  }, [reload, intervalMs, path]);

  return { data, error, loading, reload };
}
