import Link from "next/link";
import { clsx } from "clsx";

const NAV = [
  { href: "/", label: "Overview" },
  { href: "/gpus", label: "My GPUs" },
  { href: "/peers", label: "Peers" },
  { href: "/jobs", label: "Jobs" },
  { href: "/network", label: "Network" },
  { href: "/usage", label: "Usage" },
  { href: "/settings", label: "Settings" },
  { href: "/security", label: "Security" },
];

export function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div className="mx-auto flex min-h-screen max-w-6xl gap-8 px-6 py-8 md:px-10">
      <aside className="hidden w-48 shrink-0 md:block">
        <div className="sticky top-8 space-y-8">
          <div>
            <p className="font-display text-2xl tracking-tight text-accent">
              GPUMesh
            </p>
            <p className="mt-1 text-xs text-mist-500">Cloud · Phase 6</p>
          </div>
          <nav className="flex flex-col gap-1">
            {NAV.map((item) => (
              <Link
                key={item.href}
                href={item.href}
                className={clsx(
                  "rounded-lg px-3 py-2 text-sm text-mist-300 transition hover:bg-white/5 hover:text-mist-100"
                )}
              >
                {item.label}
              </Link>
            ))}
          </nav>
          <p className="text-[11px] leading-relaxed text-mist-500">
            Sync from CLI:{" "}
            <code className="text-mist-300">gpumesh sync</code>
          </p>
        </div>
      </aside>
      <main className="min-w-0 flex-1 pb-16">{children}</main>
    </div>
  );
}
