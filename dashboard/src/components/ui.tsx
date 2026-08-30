"use client";

import { clsx } from "clsx";

export function Stat({
  label,
  value,
  hint,
}: {
  label: string;
  value: string | number;
  hint?: string;
}) {
  return (
    <div className="panel p-5">
      <p className="text-[11px] font-medium uppercase tracking-wide text-mist-500">
        {label}
      </p>
      <p className="mt-2 text-3xl font-semibold tracking-tight text-mist-100">
        {value}
      </p>
      {hint ? <p className="mt-1.5 text-xs text-mist-500">{hint}</p> : null}
    </div>
  );
}

export function PageHeader({
  title,
  subtitle,
}: {
  title: string;
  subtitle?: string;
}) {
  return (
    <header className="mb-7 border-b border-line pb-5">
      <h1 className="text-2xl font-semibold tracking-tight text-mist-100">
        {title}
      </h1>
      {subtitle ? (
        <p className="mt-1.5 max-w-2xl text-sm text-mist-500">{subtitle}</p>
      ) : null}
    </header>
  );
}

export function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="panel px-6 py-14 text-center text-sm text-mist-500">
      {children}
    </div>
  );
}

export function UtilBar({ pct }: { pct: number }) {
  const w = Math.max(0, Math.min(100, pct));
  return (
    <div className="bar mt-2">
      <span style={{ width: `${w}%` }} />
    </div>
  );
}

export function Btn({
  children,
  onClick,
  disabled,
  kind = "ghost",
  type = "button",
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  kind?: "primary" | "ghost" | "danger";
  type?: "button" | "submit";
}) {
  return (
    <button
      type={type}
      disabled={disabled}
      onClick={onClick}
      className={clsx(
        "inline-flex items-center justify-center rounded-md px-3 py-1.5 text-sm font-medium transition disabled:cursor-not-allowed disabled:opacity-40",
        kind === "primary" &&
          "bg-accent text-white hover:bg-accent-dim",
        kind === "ghost" &&
          "border border-line bg-white text-mist-300 hover:bg-ink-700 hover:text-mist-100",
        kind === "danger" &&
          "border border-bad/30 bg-white text-bad hover:bg-bad-soft"
      )}
    >
      {children}
    </button>
  );
}

export function Badge({
  children,
  tone = "mist",
}: {
  children: React.ReactNode;
  tone?: "mist" | "ok" | "warn" | "bad";
}) {
  return (
    <span
      className={clsx(
        "inline-flex items-center rounded border px-1.5 py-0.5 text-[11px] font-medium",
        tone === "ok" && "border-ok/20 bg-ok-soft text-ok",
        tone === "warn" && "border-warn/20 bg-warn-soft text-warn",
        tone === "bad" && "border-bad/20 bg-bad-soft text-bad",
        tone === "mist" && "border-line bg-ink-700 text-mist-500"
      )}
    >
      {children}
    </span>
  );
}

export function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block text-sm">
      <span className="mb-1.5 block text-[11px] font-medium uppercase tracking-wide text-mist-500">
        {label}
      </span>
      {children}
    </label>
  );
}

export const inputClass =
  "w-full rounded-md border border-line bg-white px-3 py-2 text-sm text-mist-100 outline-none placeholder:text-mist-500 focus:border-accent";
