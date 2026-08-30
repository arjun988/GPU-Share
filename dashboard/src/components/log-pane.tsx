"use client";

import { useEffect, useRef, useState } from "react";
import { apiGet, type LogChunk } from "@/lib/api";

export function LogPane({
  path,
  empty = "No log yet.",
}: {
  path: string | null;
  empty?: string;
}) {
  const [text, setText] = useState("");
  const [meta, setMeta] = useState("");
  const offsetRef = useRef<number | null>(null);
  const preRef = useRef<HTMLPreElement>(null);
  const stickRef = useRef(true);

  useEffect(() => {
    offsetRef.current = null;
    setText("");
    setMeta("");
  }, [path]);

  useEffect(() => {
    if (!path) return;
    let cancelled = false;

    const tick = async () => {
      try {
        const off = offsetRef.current;
        const q = off == null ? "" : `?offset=${off}`;
        const chunk = await apiGet<LogChunk>(`${path}${q}`);
        if (cancelled) return;
        setMeta(chunk.path);
        if (off == null) {
          setText(chunk.text);
        } else if (chunk.text) {
          setText((prev) => prev + chunk.text);
        }
        offsetRef.current = chunk.offset;
      } catch {
        /* keep last */
      }
    };

    void tick();
    const t = setInterval(() => void tick(), 1500);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [path]);

  useEffect(() => {
    const el = preRef.current;
    if (el && stickRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [text]);

  if (!path) {
    return <div className="log-pane text-mist-500">{empty}</div>;
  }

  return (
    <div>
      <pre
        ref={preRef}
        className="log-pane"
        onScroll={(e) => {
          const el = e.currentTarget;
          stickRef.current =
            el.scrollHeight - el.scrollTop - el.clientHeight < 48;
        }}
      >
        {text || empty}
      </pre>
      {meta ? (
        <p className="mt-2 truncate font-mono text-[11px] text-mist-500">
          {meta}
        </p>
      ) : null}
    </div>
  );
}
