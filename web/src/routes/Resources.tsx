import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { api, type ResourceSummary } from "@/api/client";
import { ErrorState, Skeleton } from "@/components/States";

const WINDOWS: [number, string][] = [
  [6, "6 小时"],
  [24, "24 小时"],
  [24 * 7, "7 天"],
  [24 * 30, "30 天"],
];

const mib = (b: number | null | undefined) => (b == null ? "—" : `${(b / 1_048_576).toFixed(1)} MiB`);
const pct = (v: number | null | undefined) => (v == null ? "—" : `${v.toFixed(3)}%`);

/**
 * The black box (host agent, every 30 s, kept 30 days): each service's
 * memory and CPU with its extremes over a window. systemd forgets both at
 * every restart; these samples do not.
 */
export function ResourcesPanel() {
  const [hours, setHours] = useState(24);
  const q = useQuery({ queryKey: ["ops", "resources", hours], queryFn: () => api.resources(hours), refetchInterval: 60_000 });
  return (
    <section>
      <div className="mb-2 flex items-center gap-2">
        <h2 className="text-sm text-ink-muted">资源（主机代理每 30 秒记录一次，保留 30 天）</h2>
        <div className="ml-auto flex gap-1">
          {WINDOWS.map(([h, label]) => (
            <button key={h} onClick={() => setHours(h)} className={`rounded border px-2 py-0.5 text-xs ${hours === h ? "border-accent text-accent" : "border-line text-ink-muted"}`}>
              {label}
            </button>
          ))}
        </div>
      </div>
      {q.isLoading ? (
        <Skeleton rows={5} />
      ) : q.isError ? (
        <ErrorState error={q.error} what="资源记录" />
      ) : (
        <div className="overflow-x-auto rounded border border-line">
          <table className="w-full text-sm">
            <thead className="bg-surface text-left text-xs text-ink-muted">
              <tr>
                <th className="px-2 py-1 font-normal">服务</th>
                <th className="px-2 py-1 font-normal">内存 最小 / 平均 / p95 / 最大</th>
                <th className="px-2 py-1 font-normal">CPU（单核）平均 / p95 / 最大</th>
                <th className="px-2 py-1 font-normal">重启</th>
                <th className="px-2 py-1 font-normal">内存曲线</th>
              </tr>
            </thead>
            <tbody>
              {q.data!.map((r) => (
                <Row key={r.unit} r={r} hours={hours} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function Row({ r, hours }: { r: ResourceSummary; hours: number }) {
  const covered = r.first_ms ? (Date.now() - r.first_ms) / 3.6e6 : 0;
  return (
    <tr className="border-t border-line align-top">
      <td className="px-2 py-1.5 font-mono text-xs">
        {r.unit}
        {r.samples === 0 ? (
          <div className="text-warn">还没有记录</div>
        ) : covered < hours * 0.9 ? (
          <div className="text-ink-muted">记录只覆盖最近 {covered.toFixed(1)} 小时</div>
        ) : null}
      </td>
      <td className="px-2 py-1.5 font-mono text-xs tabular-nums">
        {r.memory ? `${mib(r.memory.min)} / ${mib(r.memory.avg)} / ${mib(r.memory.p95)} / ${mib(r.memory.max)}` : "—"}
      </td>
      <td className="px-2 py-1.5 font-mono text-xs tabular-nums">
        {r.cpu_percent ? `${pct(r.cpu_percent.avg)} / ${pct(r.cpu_percent.p95)} / ${pct(r.cpu_percent.max)}` : "—"}
      </td>
      <td className={`px-2 py-1.5 text-xs ${r.process_changes > 0 ? "text-warn" : "text-ink-muted"}`}>
        {r.process_changes} 次{r.samples_down > 0 ? ` · 停止 ${Math.round((r.samples_down * 30) / 60)} 分钟` : ""}
      </td>
      <td className="px-2 py-1.5">
        <Spark values={r.curve.map((p) => p.mem ?? 0)} />
      </td>
    </tr>
  );
}

function Spark({ values }: { values: number[] }) {
  if (values.length < 2) return null;
  const lo = Math.min(...values), hi = Math.max(...values), span = hi - lo || 1;
  const W = 160, H = 28;
  const d = values.map((v, i) => `${i ? "L" : "M"}${(i / (values.length - 1)) * W},${H - ((v - lo) / span) * (H - 2) - 1}`).join(" ");
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="h-7 w-40">
      <path d={d} fill="none" stroke="var(--color-accent)" strokeWidth="1" />
    </svg>
  );
}
