"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { clsx } from "clsx";
import { usePoll } from "@/lib/use-poll";
import { useTheme } from "@/lib/theme";
import type { LocalStatus } from "@/lib/api";

type NavItem = {
  href: string;
  label: string;
  icon: React.ReactNode;
};

type NavGroup = { title: string; items: NavItem[] };

const NAV: NavGroup[] = [
  {
    title: "Monitor",
    items: [
      { href: "/", label: "Overview", icon: <IconGrid /> },
      { href: "/gpus", label: "GPUs", icon: <IconChip /> },
      { href: "/usage", label: "Usage", icon: <IconChart /> },
    ],
  },
  {
    title: "Mesh",
    items: [
      { href: "/connect", label: "Connect", icon: <IconLink /> },
      { href: "/peers", label: "Peers", icon: <IconUsers /> },
      { href: "/network", label: "Network", icon: <IconNet /> },
    ],
  },
  {
    title: "Workloads",
    items: [
      { href: "/jobs", label: "Jobs", icon: <IconPlay /> },
      { href: "/logs", label: "Logs", icon: <IconList /> },
    ],
  },
  {
    title: "System",
    items: [
      { href: "/settings", label: "Settings", icon: <IconGear /> },
      { href: "/security", label: "Security", icon: <IconShield /> },
    ],
  },
];

const FLAT = NAV.flatMap((g) => g.items);

export function Shell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const { data: status } = usePoll<LocalStatus>("/v1/local/status", 5000);
  const { mode, resolved, cycle } = useTheme();
  const page =
    FLAT.find((n) =>
      n.href === "/" ? pathname === "/" : pathname.startsWith(n.href)
    )?.label ?? "Console";

  return (
    <div className="flex min-h-screen bg-canvas">
      <aside className="sticky top-0 z-20 hidden h-screen w-sidebar shrink-0 flex-col bg-surface shadow-sidebar md:flex">
        <div className="flex items-center gap-3 border-b border-line px-5 py-4">
          <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-accent text-sm font-bold text-white">
            G
          </div>
          <div className="min-w-0">
            <p className="truncate text-sm font-semibold tracking-tight text-mist-100">
              GPUMesh
            </p>
            <p className="truncate text-[11px] text-mist-500">Ops console</p>
          </div>
        </div>

        <nav className="flex-1 overflow-y-auto px-2 pb-4">
          {NAV.map((group) => (
            <div key={group.title}>
              <p className="nav-section">{group.title}</p>
              <ul className="space-y-0.5">
                {group.items.map((item) => {
                  const active =
                    item.href === "/"
                      ? pathname === "/"
                      : pathname.startsWith(item.href);
                  return (
                    <li key={item.href}>
                      <Link
                        href={item.href}
                        className={clsx(
                          "group flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm transition",
                          active
                            ? "bg-accent-soft font-medium text-accent"
                            : "text-mist-300 hover:bg-fill hover:text-mist-100"
                        )}
                      >
                        <span
                          className={clsx(
                            "flex h-7 w-7 shrink-0 items-center justify-center rounded-md border",
                            active
                              ? "border-accent/20 bg-surface text-accent"
                              : "border-line bg-fill text-mist-500 group-hover:text-mist-300"
                          )}
                        >
                          {item.icon}
                        </span>
                        {item.label}
                      </Link>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </nav>

        <div className="border-t border-line p-3">
          <div className="rounded-lg border border-line bg-fill p-3">
            <LiveDot status={status} />
            <p className="mt-2 text-[11px] text-mist-500">
              API <span className="font-mono text-mist-300">:8080</span>
            </p>
          </div>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="sticky top-0 z-10 flex h-14 items-center justify-between gap-3 border-b border-line bg-surface/95 px-4 backdrop-blur-sm md:px-6">
          <div className="flex min-w-0 items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-md bg-accent text-xs font-bold text-white md:hidden">
              G
            </div>
            <div className="min-w-0">
              <p className="truncate text-sm font-medium text-mist-100">{page}</p>
              <p className="hidden text-[11px] text-mist-500 sm:block">
                {status?.node_name
                  ? `${status.node_name} · local node`
                  : "Local operations"}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <StatusChip status={status} />
            <button
              type="button"
              onClick={cycle}
              title={`Theme: ${mode} (${resolved})`}
              className="inline-flex h-9 w-9 items-center justify-center rounded-md border border-line bg-surface text-mist-300 hover:bg-fill hover:text-mist-100"
              aria-label="Toggle theme"
            >
              {resolved === "dark" ? <IconSun /> : <IconMoon />}
            </button>
          </div>
        </header>

        <nav className="flex gap-1 overflow-x-auto border-b border-line bg-surface px-2 py-2 md:hidden">
          {FLAT.map((item) => {
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

        <main className="mx-auto w-full max-w-6xl flex-1 px-4 py-5 md:px-6 md:py-7">
          {children}
        </main>
      </div>
    </div>
  );
}

function StatusChip({ status }: { status: LocalStatus | null }) {
  if (!status?.initialized) {
    return (
      <span className="hidden rounded-md border border-line bg-fill px-2.5 py-1.5 text-[11px] text-mist-500 sm:inline">
        API offline
      </span>
    );
  }
  return (
    <span className="hidden items-center gap-2 rounded-md border border-line bg-fill px-2.5 py-1.5 text-[11px] text-mist-300 sm:inline-flex">
      <span
        className={clsx(
          "h-1.5 w-1.5 rounded-sm",
          status.share_running ? "bg-ok" : "bg-mist-500"
        )}
      />
      {status.share_running ? "Sharing" : "Idle"}
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
    <div>
      <p className="truncate text-xs font-medium text-mist-100">
        {status.node_name}
      </p>
      <p className="mt-0.5 flex items-center gap-1.5 text-[11px] text-mist-500">
        <span
          className={clsx(
            "inline-block h-1.5 w-1.5 rounded-sm",
            status.share_running ? "bg-ok" : "bg-mist-500"
          )}
        />
        {status.share_running ? "Sharing active" : "Share stopped"}
      </p>
    </div>
  );
}

function iconCls(props?: { className?: string }) {
  return clsx("h-3.5 w-3.5", props?.className);
}

function IconGrid() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <path
        d="M2.5 2.5h4v4h-4v-4Zm7 0h4v4h-4v-4Zm-7 7h4v4h-4v-4Zm7 0h4v4h-4v-4Z"
        stroke="currentColor"
        strokeWidth="1.3"
      />
    </svg>
  );
}
function IconChip() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <rect
        x="4"
        y="4"
        width="8"
        height="8"
        rx="1"
        stroke="currentColor"
        strokeWidth="1.3"
      />
      <path
        d="M6 2.5v1.5M10 2.5v1.5M6 12v1.5M10 12v1.5M2.5 6H4M2.5 10H4M12 6h1.5M12 10h1.5"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}
function IconChart() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <path
        d="M2.5 13.5h11M4.5 13V8M8 13V4.5M11.5 13V10"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}
function IconLink() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <path
        d="M6.5 9.5 9.5 6.5M7 5.2l.9-.9a2.5 2.5 0 1 1 3.5 3.5l-.9.9M9 10.8l-.9.9a2.5 2.5 0 1 1-3.5-3.5l.9-.9"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}
function IconUsers() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <circle cx="6" cy="5.5" r="2" stroke="currentColor" strokeWidth="1.3" />
      <path
        d="M2.5 13c0-2 1.6-3.5 3.5-3.5s3.5 1.5 3.5 3.5"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
      <circle cx="11" cy="6" r="1.5" stroke="currentColor" strokeWidth="1.3" />
      <path
        d="M13.5 13c0-1.5-.9-2.7-2.2-3.2"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}
function IconNet() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <circle cx="8" cy="8" r="2" stroke="currentColor" strokeWidth="1.3" />
      <circle cx="3" cy="4" r="1.2" stroke="currentColor" strokeWidth="1.2" />
      <circle cx="13" cy="4" r="1.2" stroke="currentColor" strokeWidth="1.2" />
      <circle cx="3" cy="12" r="1.2" stroke="currentColor" strokeWidth="1.2" />
      <path
        d="M4.2 4.8 6.4 7M11.8 4.8 9.6 7M4.2 11.2 6.4 9"
        stroke="currentColor"
        strokeWidth="1.2"
      />
    </svg>
  );
}
function IconPlay() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <rect
        x="2.5"
        y="3"
        width="11"
        height="10"
        rx="1.5"
        stroke="currentColor"
        strokeWidth="1.3"
      />
      <path d="M7 6.2v3.6L10.2 8 7 6.2Z" fill="currentColor" />
    </svg>
  );
}
function IconList() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <path
        d="M4 4h9M4 8h9M4 12h6"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
      <path
        d="M2.5 4h.01M2.5 8h.01M2.5 12h.01"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  );
}
function IconGear() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <circle cx="8" cy="8" r="2" stroke="currentColor" strokeWidth="1.3" />
      <path
        d="M8 2.5v1.5M8 12v1.5M2.5 8H4M12 8h1.5M4.2 4.2l1.1 1.1M10.7 10.7l1.1 1.1M11.8 4.2l-1.1 1.1M5.3 10.7l-1.1 1.1"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}
function IconShield() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className={iconCls()} aria-hidden>
      <path
        d="M8 2.5 13 4.5v3.2c0 3.2-2.1 5.3-5 6.3-2.9-1-5-3.1-5-6.3V4.5L8 2.5Z"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
      />
    </svg>
  );
}
function IconSun() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className="h-4 w-4" aria-hidden>
      <circle cx="8" cy="8" r="2.5" stroke="currentColor" strokeWidth="1.3" />
      <path
        d="M8 2v1.5M8 12.5V14M2 8h1.5M12.5 8H14M3.8 3.8l1.1 1.1M11.1 11.1l1.1 1.1M12.2 3.8l-1.1 1.1M4.9 11.1l-1.1 1.1"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}
function IconMoon() {
  return (
    <svg viewBox="0 0 16 16" fill="none" className="h-4 w-4" aria-hidden>
      <path
        d="M12.5 9.2A4.7 4.7 0 0 1 6.8 3.5 4.8 4.8 0 1 0 12.5 9.2Z"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
      />
    </svg>
  );
}
