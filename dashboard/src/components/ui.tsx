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
      <p className="text-xs uppercase tracking-wider text-mist-500">{label}</p>
      <p className="mt-2 font-display text-3xl text-mist-100">{value}</p>
      {hint ? <p className="mt-1 text-xs text-mist-500">{hint}</p> : null}
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
    <header className="mb-8">
      <h1 className="font-display text-3xl text-mist-100">{title}</h1>
      {subtitle ? (
        <p className="mt-2 max-w-xl text-sm text-mist-500">{subtitle}</p>
      ) : null}
    </header>
  );
}

export function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="panel px-6 py-12 text-center text-sm text-mist-500">
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
