import { useQuery } from "@tanstack/react-query";

import { api, type Capabilities } from "@/api/client";

export function Overview() {
  const { data: caps } = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const { data: listing } = useQuery({
    queryKey: ["runs"],
    queryFn: api.runs,
    enabled: caps?.runs.available === true,
  });

  return (
    <div>
      <h1 className="mb-4 text-lg text-ink">总览</h1>
      <div className="grid gap-3 sm:grid-cols-3">
        <Tile label="运行记录" value={listing ? String(listing.entries.length) : "—"} />
        <Tile
          label="合计已实现盈亏"
          value={listing ? listing.total_pnl.toFixed(3) : "—"}
        />
        <Tile
          label="写入模式"
          value={caps?.writes.available ? "已开启" : "只读"}
          sub={caps?.writes.available ? undefined : caps?.writes.reason}
        />
      </div>
      {caps && <Unavailable caps={caps} />}
    </div>
  );
}

/**
 * What this deck cannot do, and why. Shown rather than hidden: an
 * operator who cannot find a feature should learn here that it does not
 * exist yet, instead of concluding the console is broken.
 */
function Unavailable({ caps }: { caps: Capabilities }) {
  const off = (Object.entries(caps) as [string, unknown][]).filter(
    ([key, value]) =>
      key !== "version" &&
      typeof value === "object" &&
      value !== null &&
      (value as { available: boolean }).available === false,
  ) as [string, { reason: string }][];

  if (off.length === 0) return null;
  return (
    <section className="mt-8">
      <h2 className="mb-2 text-sm text-ink-muted">本 deck 暂不支持</h2>
      <ul className="space-y-1 text-sm">
        {off.map(([name, capability]) => (
          <li key={name} className="flex gap-3">
            <span className="w-28 shrink-0 font-mono text-xs text-ink-muted">{name}</span>
            <span className="text-ink-muted">{capability.reason}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

function Tile({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="rounded border border-line bg-surface p-4">
      <div className="text-xs text-ink-muted">{label}</div>
      <div className="mt-1 text-xl text-ink">{value}</div>
      {sub && <div className="mt-1 text-xs text-ink-muted">{sub}</div>}
    </div>
  );
}
