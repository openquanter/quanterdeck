import { useQuery } from "@tanstack/react-query";

import { api, type Capabilities } from "@/api/client";

export function Overview() {
  const { data: caps, isError: capsFailed } = useQuery({
    queryKey: ["capabilities"],
    queryFn: api.capabilities,
  });
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
          value={listing && listing.total_pnl !== null ? listing.total_pnl.toFixed(3) : "—"}
          sub={
            listing && listing.total_pnl === null
              ? "无法合计：有记录未能读取，或混有不同类型的运行"
              : undefined
          }
        />
        {/* Unknown until the deck says. A failed request rendered as
            "只读" was a definite answer to a question nobody had asked. */}
        <Tile
          label="写入模式"
          value={caps ? (caps.writes.available ? "已开启" : "只读") : "—"}
          sub={
            capsFailed
              ? "无法获取能力信息"
              : caps?.writes.available
                ? undefined
                : caps?.writes.reason
          }
        />
      </div>
      {caps?.ops.available && <OpsSummary />}
      {caps && <Unavailable caps={caps} />}
    </div>
  );
}

/** The trading host at a glance, when the deck has its agent. */
function OpsSummary() {
  const status = useQuery({ queryKey: ["ops", "status"], queryFn: api.traderStatus, refetchInterval: 10_000, retry: false });
  const alerts = useQuery({ queryKey: ["ops", "alerts"], queryFn: api.alerts, refetchInterval: 10_000 });
  const live = useQuery({ queryKey: ["live", "latest"], queryFn: api.liveLatest, refetchInterval: 30_000, retry: false });
  const verdict = live.data?.reconciliation.verdict;
  return (
    <section className="mt-6">
      <h2 className="mb-2 text-sm text-ink-muted">交易主机</h2>
      <div className="grid gap-3 sm:grid-cols-4">
        <Tile
          label="交易进程"
          value={status.isError ? "无法获取" : status.data ? (status.data.halted ? "已停机" : "交易中") : "—"}
          sub={status.data ? `${status.data.symbol} · 挂单 ${status.data.resting} · ${status.data.positions.map((p) => `${p.side} ${p.amount}`).join(" ") || "无持仓"}${status.data.pnl ? ` · 本次运行盈亏 ${status.data.pnl.net}` : ""}` : undefined}
        />
        <Tile
          label="告警"
          value={alerts.data ? (alerts.data.length ? `${alerts.data.length} 条` : "无") : "—"}
          sub={alerts.data?.[0]?.message}
        />
        <Tile
          label="实盘对账"
          value={verdict === "agree" ? "一致" : verdict === "disagree" ? "不一致" : verdict === "cannot_tell" ? "无法判断" : "—"}
          sub={live.data ? `交易所读数 ${Math.round(live.data.record_age_ms / 1000)} 秒前` : undefined}
        />
        <Tile
          label="持仓核对（进程内）"
          value={status.data?.reconcile.agreed === true ? "一致" : status.data?.reconcile.agreed === false ? "不一致" : "尚未核对"}
          sub={status.data ? `累计不一致 ${status.data.reconcile.mismatches}` : undefined}
        />
      </div>
    </section>
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
