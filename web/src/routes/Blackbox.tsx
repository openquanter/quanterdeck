import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { api, type BlackboxWindow } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";

import { RecordTable } from "./Live";

const PRESETS: [number, string][] = [
  [1, "1 小时"],
  [6, "6 小时"],
  [24, "24 小时"],
  [72, "3 天"],
  [168, "7 天"],
];

const EVENT_NAMES: Record<string, string> = {
  unit_started: "服务启动",
  unit_stopped: "服务停止",
  trader_halted: "交易进程停机",
  trader_resumed: "交易进程解除停机",
  control_lost: "控制口无应答",
  control_back: "控制口恢复",
  alert_raised: "告警触发",
  alert_cleared: "告警恢复",
  recording_started: "主机代理启动，开始记录（此前的空白没有记录）",
};

const fmtTime = (ms: number) => new Date(ms).toLocaleString("zh-CN", { hour12: false });
const toLocalInput = (ms: number) => {
  const d = new Date(ms - new Date().getTimezoneOffset() * 60_000);
  return d.toISOString().slice(0, 16);
};

/**
 * Review (复盘): what the system, each service and the trader were doing
 * over a window, every event in it, and any moment opened up — the
 * snapshot at that time, the trader's decisions and fills around it, and
 * its own output around it. All of it recorded as it happened; nothing
 * here is reconstructed after the fact.
 */
export function Blackbox() {
  const [hours, setHours] = useState(6);
  const [end, setEnd] = useState<number | null>(null);
  const to = end ?? Math.floor(Date.now() / 60_000) * 60_000;
  const from = to - hours * 3_600_000;
  const q = useQuery({ queryKey: ["blackbox", from, to], queryFn: () => api.blackbox(from, to, 400), refetchInterval: end ? false : 60_000 });
  const [moment, setMoment] = useState<number | null>(null);

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <h1 className="text-lg text-ink">黑匣子 · 复盘</h1>
        <div className="ml-auto flex flex-wrap items-center gap-1 text-xs">
          {PRESETS.map(([h, label]) => (
            <button key={h} onClick={() => setHours(h)} className={`rounded border px-2 py-0.5 ${hours === h ? "border-accent text-accent" : "border-line text-ink-muted"}`}>
              {label}
            </button>
          ))}
          <span className="ml-2 text-ink-muted">截止</span>
          <input
            type="datetime-local"
            className="rounded border border-line bg-ground px-1 py-0.5"
            value={toLocalInput(to)}
            onChange={(e) => setEnd(e.target.value ? new Date(e.target.value).getTime() : null)}
          />
          {end && (
            <button className="text-accent" onClick={() => setEnd(null)}>
              回到现在
            </button>
          )}
        </div>
      </div>
      <p className="text-xs text-ink-muted">
        主机代理每 30 秒记录一次系统、各服务和交易进程的状态，保留 90 天；状态变化和告警记为事件。点曲线或事件可以打开那一刻。
      </p>

      {q.isLoading ? (
        <Skeleton rows={10} />
      ) : q.isError ? (
        <ErrorState error={q.error} what="黑匣子记录" />
      ) : !q.data || Object.keys(q.data.units).length === 0 ? (
        <Empty title="这段时间没有记录。" next="黑匣子从主机代理这次部署开始记录；更早的时间没有数据。" />
      ) : (
        <Window w={q.data} onPick={setMoment} moment={moment} />
      )}

      {moment !== null && <Moment at={moment} onClose={() => setMoment(null)} />}
    </div>
  );
}

function Window({ w, onPick, moment }: { w: BlackboxWindow; onPick: (t: number) => void; moment: number | null }) {
  const units = Object.entries(w.units);
  const events = w.events;
  const halts = useMemo(() => {
    // Halted spans, from the trader samples.
    const spans: [number, number][] = [];
    let start: number | null = null;
    for (const t of w.trader) {
      if (t.trader.halted && start === null) start = t.at;
      if (!t.trader.halted && start !== null) {
        spans.push([start, t.at]);
        start = null;
      }
    }
    if (start !== null) spans.push([start, w.to_ms]);
    return spans;
  }, [w]);
  const axis = { from: w.from_ms, to: w.to_ms, events: events.map((e) => e.at), halts, moment };
  const mib = (b: number | null | undefined) => (b == null ? null : b / 1_048_576);

  return (
    <div className="space-y-4">
      <Chart
        title="系统：负载（1 分钟）与可用内存 %"
        axis={axis}
        onPick={onPick}
        series={[
          { name: "负载", points: w.host.map((h) => [h.at, h.host.load?.[0] ?? null]) },
          {
            name: "可用内存 %",
            points: w.host.map((h) => [h.at, h.host.mem_total ? ((h.host.mem_available ?? 0) / h.host.mem_total) * 100 : null]),
          },
        ]}
      />
      <Chart
        title="系统：资源压力（等待 CPU / 内存 / IO 的时间占比，10 秒平均 %）"
        axis={axis}
        onPick={onPick}
        series={[
          { name: "CPU", points: w.host.map((h) => [h.at, h.host.psi_cpu ?? null]) },
          { name: "内存", points: w.host.map((h) => [h.at, h.host.psi_memory ?? null]) },
          { name: "IO", points: w.host.map((h) => [h.at, h.host.psi_io ?? null]) },
        ]}
      />
      <Chart
        title="各服务内存（MiB）"
        axis={axis}
        onPick={onPick}
        series={units.map(([u, v]) => ({ name: u.replace(".service", ""), points: v.curve.map((p) => [p.at, mib(p.mem)] as [number, number | null]) }))}
      />
      <Chart
        title="各服务 CPU（单核 %）"
        axis={axis}
        onPick={onPick}
        series={units.map(([u, v]) => ({ name: u.replace(".service", ""), points: v.curve.map((p) => [p.at, p.cpu] as [number, number | null]) }))}
      />
      <Chart
        title="交易进程：挂单数与累计 tick（灰底为停机）"
        axis={axis}
        onPick={onPick}
        series={[
          { name: "挂单", points: w.trader.map((t) => [t.at, t.trader.resting ?? null]) },
          { name: "tick", points: w.trader.map((t) => [t.at, t.trader.ticks ?? null]) },
        ]}
      />

      <table className="w-full text-sm">
        <thead className="text-left text-xs text-ink-muted">
          <tr>
            <th className="py-1 font-normal">服务</th>
            <th className="font-normal">内存 最小 / 平均 / 最大</th>
            <th className="font-normal">CPU 平均 / p95 / 最大</th>
            <th className="font-normal">停止的采样</th>
          </tr>
        </thead>
        <tbody>
          {units.map(([u, v]) => (
            <tr key={u} className="border-t border-line font-mono text-xs">
              <td className="py-1">{u}</td>
              <td>{v.memory ? `${mib(v.memory.min)!.toFixed(1)} / ${mib(v.memory.avg)!.toFixed(1)} / ${mib(v.memory.max)!.toFixed(1)} MiB` : "—"}</td>
              <td>{v.cpu_percent ? `${v.cpu_percent.avg.toFixed(3)} / ${v.cpu_percent.p95.toFixed(3)} / ${v.cpu_percent.max.toFixed(3)}%` : "—"}</td>
              <td className={v.samples_down ? "text-warn" : "text-ink-muted"}>{v.samples_down}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <section>
        <h2 className="mb-1 text-sm text-ink-muted">事件（{events.length}）</h2>
        {events.length === 0 ? (
          <p className="text-xs text-ink-muted">这段时间没有状态变化、停机或告警。</p>
        ) : (
          <ul className="max-h-72 space-y-0.5 overflow-auto text-xs">
            {[...events].reverse().map((e, k) => (
              <li key={k}>
                <button className="text-left hover:text-ink" onClick={() => onPick(e.at)}>
                  <span className="font-mono text-ink-muted">{fmtTime(e.at)}</span>{" "}
                  <span className={/stopped|halted|lost|raised/.test(e.what) ? "text-warn" : "text-ink"}>{EVENT_NAMES[e.what] ?? e.what}</span>{" "}
                  <span className="text-ink-muted">
                    {[e.unit, e.message, e.reason, e.result && `结果 ${e.result}`, e.exit_status && `退出码 ${e.exit_status}`].filter(Boolean).join(" · ")}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

const COLORS = ["var(--color-accent)", "var(--color-ink)", "var(--color-warn)", "var(--color-ink-muted)", "#8b5cf6"];

function Chart({
  title,
  series,
  axis,
  onPick,
}: {
  title: string;
  series: { name: string; points: [number, number | null][] }[];
  axis: { from: number; to: number; events: number[]; halts: [number, number][]; moment: number | null };
  onPick: (t: number) => void;
}) {
  const [hover, setHover] = useState<number | null>(null);
  const W = 900, H = 110, pad = 4;
  const all = series.flatMap((s) => s.points.map((p) => p[1]).filter((v): v is number => v !== null));
  if (all.length === 0) {
    return (
      <div className="rounded border border-line p-2 text-xs text-ink-muted">
        {title}：这段时间没有数据
      </div>
    );
  }
  const lo = Math.min(0, ...all), hi = Math.max(...all), span = hi - lo || 1;
  const x = (t: number) => ((t - axis.from) / (axis.to - axis.from)) * W;
  const y = (v: number) => H - pad - ((v - lo) / span) * (H - 2 * pad);
  const tAt = (px: number) => axis.from + (px / W) * (axis.to - axis.from);
  const valueAt = (pts: [number, number | null][], t: number) => {
    let best: [number, number | null] | null = null;
    for (const p of pts) if (p[0] <= t) best = p;
    return best?.[1] ?? null;
  };
  return (
    <div className="rounded border border-line bg-surface p-2">
      <div className="mb-1 flex flex-wrap gap-3 text-xs text-ink-muted">
        <span className="text-ink">{title}</span>
        {series.map((s, i) => (
          <span key={s.name} style={{ color: COLORS[i % COLORS.length] }}>
            ■ {s.name}
            {hover !== null && `：${valueAt(s.points, hover)?.toFixed(3) ?? "—"}`}
          </span>
        ))}
        <span className="ml-auto">最大 {hi.toFixed(2)}</span>
        {hover !== null && <span>{fmtTime(hover)}</span>}
      </div>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className="h-28 w-full cursor-crosshair"
        preserveAspectRatio="none"
        onMouseMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setHover(tAt(((e.clientX - r.left) / r.width) * W));
        }}
        onMouseLeave={() => setHover(null)}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          onPick(Math.round(tAt(((e.clientX - r.left) / r.width) * W)));
        }}
      >
        {axis.halts.map(([a, b], k) => (
          <rect key={k} x={x(a)} y={0} width={Math.max(x(b) - x(a), 1)} height={H} fill="var(--color-line)" opacity={0.6} />
        ))}
        {axis.events.map((t, k) => (
          <line key={k} x1={x(t)} x2={x(t)} y1={0} y2={H} stroke="var(--color-warn)" strokeWidth={0.5} opacity={0.6} />
        ))}
        {series.map((s, i) => {
          // Break the line across a stretch nothing was recorded in, so a
          // gap reads as a gap and not as a steady value.
          const steps = s.points.slice(1).map((p, k) => p[0] - s.points[k][0]).sort((a, b) => a - b);
          const gap = 3 * (steps[Math.floor(steps.length / 2)] ?? Infinity);
          let d = "";
          let pen = false;
          let prev = -Infinity;
          for (const [t, v] of s.points) {
            if (t - prev > gap) pen = false;
            prev = t;
            if (v === null) {
              pen = false;
              continue;
            }
            d += `${pen ? "L" : "M"}${x(t).toFixed(1)},${y(v).toFixed(1)}`;
            pen = true;
          }
          return <path key={s.name} d={d} fill="none" stroke={COLORS[i % COLORS.length]} strokeWidth={1.2} vectorEffect="non-scaling-stroke" />;
        })}
        {axis.moment !== null && <line x1={x(axis.moment)} x2={x(axis.moment)} y1={0} y2={H} stroke="var(--color-accent)" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />}
        {hover !== null && <line x1={x(hover)} x2={x(hover)} y1={0} y2={H} stroke="var(--color-ink-muted)" strokeWidth={0.5} vectorEffect="non-scaling-stroke" />}
      </svg>
    </div>
  );
}

/** One moment, opened up. */
function Moment({ at, onClose }: { at: number; onClose: () => void }) {
  const snap = useQuery({ queryKey: ["blackbox", "at", at], queryFn: () => api.blackboxAt(at) });
  const journals = useQuery({ queryKey: ["journals"], queryFn: api.journals });
  // The journal of the run that was going at that moment: the latest one
  // started before it (ids carry the start time).
  const journalId = useMemo(() => {
    const stamp = (id: string) => {
      const m = id.match(/(\d{8})-(\d{6})$/);
      if (!m) return 0;
      const [, d, t] = m;
      return new Date(`${d.slice(0, 4)}-${d.slice(4, 6)}-${d.slice(6)}T${t.slice(0, 2)}:${t.slice(2, 4)}:${t.slice(4)}`).getTime();
    };
    return (journals.data ?? []).map((j) => j.id).filter((id) => stamp(id) <= at).sort((a, b) => stamp(b) - stamp(a))[0];
  }, [journals.data, at]);
  const window = 2 * 60_000;
  const records = useQuery({
    queryKey: ["records", journalId, "around", at],
    queryFn: () => api.recordsBetween(journalId as string, ["submitted", "outcome", "fill", "cancelled", "refused", "operator", "reconciled"], 300, (at - window) * 1e6, (at + window) * 1e6),
    enabled: Boolean(journalId),
  });
  const logs = useQuery({ queryKey: ["journal-log", "oqp-live", at], queryFn: () => api.journalLog("oqp-live.service", at - window, at + window, 400) });
  const s = snap.data;
  const t = s?.trader?.trader;
  const h = s?.host?.host;

  return (
    <section className="space-y-3 rounded border border-accent p-4">
      <div className="flex items-center">
        <h2 className="text-base text-ink">{fmtTime(at)} 这一刻</h2>
        <button className="ml-auto text-xs text-ink-muted hover:text-ink" onClick={onClose}>
          关闭
        </button>
      </div>
      {snap.isLoading ? (
        <Skeleton rows={3} />
      ) : snap.isError ? (
        <ErrorState error={snap.error} what="那一刻的记录" />
      ) : (
        <div className="grid gap-3 text-xs sm:grid-cols-3">
          <div className="rounded border border-line bg-surface p-2">
            <div className="mb-1 text-ink-muted">交易进程</div>
            {t ? (
              <div className="space-y-0.5 font-mono">
                <div className={t.halted ? "text-bad" : "text-ink"}>{t.halted ? `已停机：${t.halt_reason ?? ""}` : "交易中"}</div>
                <div>持仓 {(t.positions ?? []).map((p: { side: string; amount: string }) => `${p.side} ${p.amount}`).join(" ") || "无"}</div>
                <div>挂单 {t.resting} · tick {t.ticks}</div>
                <div>核对 {t.reconcile?.agreed === true ? "一致" : t.reconcile?.agreed === false ? "不一致" : "尚未"} · 读不出 {t.feed?.unreadable}</div>
                {t.journal_lost && <div className="text-bad">日志无法写入：{t.journal_lost}</div>}
              </div>
            ) : (
              <div className="text-warn">那一刻前后 10 分钟没有交易进程的记录（控制口无应答或进程未运行）。</div>
            )}
          </div>
          <div className="rounded border border-line bg-surface p-2">
            <div className="mb-1 text-ink-muted">系统</div>
            {h ? (
              <div className="space-y-0.5 font-mono">
                <div>负载 {h.load?.join(" / ")}</div>
                <div>可用内存 {h.mem_available ? (h.mem_available / 2 ** 30).toFixed(1) : "—"} / {h.mem_total ? (h.mem_total / 2 ** 30).toFixed(1) : "—"} GiB</div>
                <div>压力 CPU {h.psi_cpu} · 内存 {h.psi_memory} · IO {h.psi_io}</div>
                <div className={h.clock_synced ? "" : "text-warn"}>时钟 {h.clock_synced ? "已同步" : "未同步"}</div>
              </div>
            ) : (
              <div className="text-warn">没有记录。</div>
            )}
          </div>
          <div className="rounded border border-line bg-surface p-2">
            <div className="mb-1 text-ink-muted">服务</div>
            <div className="space-y-0.5 font-mono">
              {Object.entries(s?.units ?? {}).map(([u, v]) => (
                <div key={u} className={v.active ? "" : "text-warn"}>
                  {u.replace(".service", "")} {v.active ? `${((v.mem ?? 0) / 1_048_576).toFixed(1)} MiB · ${v.tasks} 任务` : "未运行"}
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      <div>
        <h3 className="mb-1 text-xs text-ink-muted">前后 2 分钟的决策与成交（{journalId ?? "没有那时的 journal"}）</h3>
        {!journalId ? null : records.isLoading ? <Skeleton rows={3} /> : records.isError ? <ErrorState error={records.error} what="交易日志" /> : <RecordTable page={records.data!} emptyNext="这 4 分钟里交易进程没有下单、成交或撤单。" />}
      </div>
      <div>
        <h3 className="mb-1 text-xs text-ink-muted">前后 2 分钟交易进程的输出（systemd journal，带时间戳）</h3>
        {logs.isLoading ? (
          <Skeleton rows={3} />
        ) : logs.isError ? (
          <ErrorState error={logs.error} what="程序输出" />
        ) : logs.data!.lines.length === 0 ? (
          <p className="text-xs text-ink-muted">这 4 分钟没有输出；输出改写进 systemd journal 之前的时段只有不带时间的日志文件，在「日志」页查看。</p>
        ) : (
          <pre className="max-h-80 overflow-auto rounded border border-line bg-ground p-2 font-mono text-xs">{logs.data!.lines.join("\n")}</pre>
        )}
      </div>
    </section>
  );
}
