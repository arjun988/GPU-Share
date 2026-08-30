"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { clsx } from "clsx";
import { usePoll } from "@/lib/use-poll";
import type { LocalStatus } from "@/lib/api";

const NAV = [
  { href: "/", label: "Overview" },
  { href: "/connect", label: "Connect" },
  { href: "/gpus", label: "GPUs" },
  { href: "/peers", label: "Peers" },
  { href: "/jobs", label: "Jobs" },
  { href: "/logs", label: "Logs" },
  { href: "/network", label: "Network" },
  { href: "/settings", label: "Settings" },
  { href: "/security", label: "Security" },
];

export function Shell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const { data: status } = usePoll<LocalStatus>("/v1/local/status", 5000);
  const page =
    NAV.find((n) =>
      n.href === "/" ? pathname === "/" : pathname.startsWith(n.href)
    )?.label ?? "Console";

  return (
    <div className="flex min-h-screen bg-canvas">
      <aside className="sticky top-0 hidden h-screen w-56 shrink-0 flex-col border-r border-line bg-white md:flex">
        <div className="border-b border-line px-5 py-5">
          <p className="text-lg font-semibold tracking-tight text-mist-100">
            GPUMesh
          </p>
          <p className="mt-0.5 text-xs text-mist-500">Operations console</p>
        </div>
        <nav className="flex flex-1 flex-col gap-0.5 p-3">
          {NAV.map((item) => {
            const active =
              item.href === "/"
                ? pathname === "/"
                : pathname.startsWith(item.href);
            return (
              <Link
                key={item.href}
                href={item.href}
                className={clsx(
                  "rounded-md px-3 py-2 text-sm transition",
                  active
                    ? "bg-accent-soft font-medium text-accent"
                    : "text-mist-300 hover:bg-ink-700 hover:text-mist-100"
                )}
              >
                {item.label}
              </Link>
            );
          })}
        </nav>
        <div className="border-t border-line px-5 py-4">
          <LiveDot status={status} />
          <p className="mt-3 text-[11px] leading-relaxed text-mist-500">
            Control plane{" "}
            <code className="!bg-transparent !p-0 text-mist-300">:8080</code>
          </p>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="sticky top-0 z-10 flex h-14 items-center justify-between border-b border-line bg-white px-5 md:px-8">
          <div className="flex items-center gap-3">
            <p className="text-sm font-medium text-mist-100 md:hidden">
              GPUMesh
            </p>
            <span className="hidden text-sm text-mist-500 md:inline">
              {page}
            </span>
          </div>
          <StatusChip status={status} />
        </header>

        <nav className="flex gap-1 overflow-x-auto border-b border-line bg-white px-3 py-2 md:hidden">
          {NAV.map((item) => {
            const active =
              item.href === "/"
                ? pathname === "/"
                : pathname.startsWith(item.href);
            return (
              <Link
                key={item.href}
                href={item.href}
                className={clsx(
                  "shrink-0 rounded-md px-3 py-1.5 text-xs",
                  active
                    ? "bg-accent-soft font-medium text-accent"
                    : "text-mist-300"
                )}
              >
                {item.label}
              </Link>
            );
          })}
        </nav>

        <main className="mx-auto w-full max-w-6xl flex-1 px-5 py-6 md:px-8 md:py-8">
          {children}
        </main>
      </div>
    </div>
  );
}

function StatusChip({ status }: { status: LocalStatus | null }) {
  if (!status?.initialized) {
    return (
      <span className="rounded-md border border-line bg-ink-700 px-2.5 py-1 text-[11px] text-mist-500">
        API offline / not initialized
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-2 rounded-md border border-line bg-ink-700 px-2.5 py-1 text-[11px] text-mist-300">
      <span
        className={clsx(
          "h-1.5 w-1.5 rounded-sm",
          status.share_running ? "bg-ok" : "bg-mist-500"
        )}
      />
      {status.node_name}
      {status.share_running ? " · sharing" : " · idle"}
    </span>
  );
}

function LiveDot({ status }: { status: LocalStatus | null }) {
  if (!status) {
    return <p className="text-[11px] text-mist-500">Waiting for API…</p>;
  }
  if (!status.initialized) {
    return (
      <p className="text-[11px] text-mist-500">
        Run <code className="!bg-transparent !p-0">gpumesh init</code>
      </p>
    );
  }
  return (
    <p className="text-[11px] text-mist-300">
      <span
        className={clsx(
          "mr-1.5 inline-block h-1.5 w-1.5 rounded-sm align-middle",
          status.share_running ? "bg-ok" : "bg-mist-500"
        )}
      />
      {status.node_name}
    </p>
  );
}
