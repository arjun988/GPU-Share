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
      <p className="mt-2 text-2xl font-semibold tracking-tight text-mist-100 tabular-nums sm:text-3xl">
        {value}
      </p>
      {hint ? <p className="mt-1.5 text-xs text-mist-500">{hint}</p> : null}
    </div>
  );
}

export function PageHeader({
  title,
  subtitle,
  actions,
}: {
  title: string;
  subtitle?: string;
  actions?: React.ReactNode;
}) {
  return (
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
      <div className="min-w-0">
        <h1 className="text-xl font-semibold tracking-tight text-mist-100 sm:text-2xl">
          {title}
        </h1>
        {subtitle ? (
          <p className="mt-1 max-w-2xl text-sm text-mist-500">{subtitle}</p>
        ) : null}
      </div>
      {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
    </header>
  );
}

export function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="panel px-6 py-16 text-center text-sm text-mist-500">
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
  className,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  kind?: "primary" | "ghost" | "danger";
  type?: "button" | "submit";
  className?: string;
}) {
  return (
    <button
      type={type}
      disabled={disabled}
      onClick={onClick}
      className={clsx(
        "inline-flex h-9 items-center justify-center gap-1.5 rounded-md px-3.5 text-sm font-medium transition disabled:cursor-not-allowed disabled:opacity-40",
        kind === "primary" &&
          "bg-accent text-white hover:bg-accent-dim",
        kind === "ghost" &&
          "border border-line bg-surface text-mist-300 hover:bg-fill hover:text-mist-100",
        kind === "danger" &&
          "border border-bad/30 bg-surface text-bad hover:bg-bad-soft",
        className
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
        "inline-flex items-center rounded-md border px-1.5 py-0.5 text-[11px] font-medium",
        tone === "ok" && "border-ok/20 bg-ok-soft text-ok",
        tone === "warn" && "border-warn/20 bg-warn-soft text-warn",
        tone === "bad" && "border-bad/20 bg-bad-soft text-bad",
        tone === "mist" && "border-line bg-fill text-mist-500"
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

export function CardHeader({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 className="section-title">{title}</h2>
        {description ? (
          <p className="mt-0.5 text-xs text-mist-500">{description}</p>
        ) : null}
      </div>
      {action}
    </div>
  );
}

export const inputClass =
  "w-full rounded-md border border-line bg-surface px-3 py-2 text-sm text-mist-100 outline-none placeholder:text-mist-500 focus:border-accent";
