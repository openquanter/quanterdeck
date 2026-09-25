import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Activity, Cpu, History, RotateCcw, Server, Terminal } from "lucide-react";

import { api, type BlackboxEvent, type BlackboxWindow } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { type Point, TimeSeries } from "@/ui/charts";
import { Badge, Button, Card, Drawer, Freshness, KV, PageHeader, Segmented, StatusDot, Table, fmtBytes, fmtTime, type Tone } from "@/ui/kit";

import { RecordRows } from "./Trading";

const PRESETS: { value: number; label: string }[] = [
  { value: 1, label: "1 小时" },
  { value: 6, label: "6 小时" },
  { value: 24, label: "24 小时" },
  { value: 72, label: "3 天" },
  { value: 168, label: "7 天" },
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

/** An event's state colour: trouble starting is amber, trouble ending is green, the rest neutral. */
function eventTone(what: string): Tone {
  if (/stopped|halted|lost|raised/.test(what)) return "warn";
  if (/started|resumed|back|cleared/.test(what) && what !== "recording_started") return "good";
  return "neutral";
}

const WARN_MARK = "#e8a83e";
const PLAIN_MARK = "#6b7383";
const MOMENT_MARK = "#5b9bff";

const toLocalInput = (ms: number) => {
  const d = new Date(ms - new Date().getTimezoneOffset() * 60_000);
  return d.toISOString().slice(0, 16);
};

/** How much of the window on either side of an opened moment to show. */
const AROUND_MS = 2 * 60_000;

/**
 * Review (黑匣子复盘, docs/UI-V4 §4.5): what the system, each service
 * and the trader were doing over a window, every event in it, and any
 * moment opened up in a drawer — the snapshot at that time, the trader's
 * decisions and fills around it, and its own output around it. All of
 * it recorded as it happened; nothing here is reconstructed after the
 * fact.
 */
export function Blackbox() {
  const [hours, setHours] = useState(6);
  const [end, setEnd] = useState<number | null>(null);
  const to = end ?? Math.floor(Date.now() / 60_000) * 60_000;
  const from = to - hours * 3_600_000;
  const q = useQuery({ queryKey: ["blackbox", from, to], queryFn: () => api.blackbox(from, to, 400), refetchInterval: end ? false : 60_000 });
  const [moment, setMoment] = useState<number | null>(null);

  return (
    <div className="space-y-5">
      <PageHeader
        title="黑匣子复盘"
        description="主机代理每 30 秒记录一次系统、各服务和交易进程的状态，保留 90 天；状态变化和告警记为事件。点曲线上的点或事件，打开那一刻。"
        meta={end === null && <Freshness at={q.dataUpdatedAt} fetching={q.isFetching} staleAfterS={120} onRefresh={() => q.refetch()} />}
        actions={
          <>
            <Segmented value={hours} options={PRESETS} onChange={setHours} />
            <label className="flex items-center gap-1.5 text-xs text-ink-muted">
              截止
              <input
                type="datetime-local"
                className="h-8 rounded-md border border-line-strong bg-surface-raised px-2 text-xs text-ink outline-none focus:border-accent"
                value={toLocalInput(to)}
                onChange={(e) => setEnd(e.target.value ? new Date(e.target.value).getTime() : null)}
              />
            </label>
            {end !== null && (
              <Button size="sm" variant="ghost" icon={<RotateCcw className="h-3.5 w-3.5" />} onClick={() => setEnd(null)}>
                回到现在
              </Button>
            )}
          </>
        }
      />

      {q.isLoading ? (
        <Skeleton rows={10} />
      ) : q.isError ? (
        <ErrorState error={q.error} what="黑匣子记录" />
      ) : !q.data || Object.keys(q.data.units).length === 0 ? (
        <Empty title="这段时间没有记录。" next="黑匣子从主机代理这次部署开始记录；更早的时间没有数据。换一个时间范围，或点「回到现在」。" />
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
  const marks = useMemo(
    () => [
      ...events.map((e) => ({ at: e.at, label: EVENT_NAMES[e.what] ?? e.what, color: eventTone(e.what) === "warn" ? WARN_MARK : PLAIN_MARK })),
      ...(moment !== null ? [{ at: moment, label: "打开的时刻", color: MOMENT_MARK }] : []),
    ],
    [events, moment],
  );
  const mib = (b: number | null | undefined) => (b == null ? null : b / 1_048_576);

  const charts = useMemo(
    () => ({
      load: [{ name: "负载", points: w.host.map((h) => [h.at, h.host.load?.[0] ?? null] as Point), area: true }],
      mem: [
        {
          name: "可用内存",
          points: w.host.map((h) => [h.at, h.host.mem_total ? ((h.host.mem_available ?? 0) / h.host.mem_total) * 100 : null] as Point),
          area: true,
        },
      ],
      psi: [
        { name: "CPU", points: w.host.map((h) => [h.at, h.host.psi_cpu ?? null] as Point) },
        { name: "内存", points: w.host.map((h) => [h.at, h.host.psi_memory ?? null] as Point) },
        { name: "IO", points: w.host.map((h) => [h.at, h.host.psi_io ?? null] as Point) },
      ],
      unitMem: Object.entries(w.units).map(([u, v]) => ({ name: u.replace(".service", ""), points: v.curve.map((p) => [p.at, mib(p.mem)] as Point) })),
      unitCpu: Object.entries(w.units).map(([u, v]) => ({ name: u.replace(".service", ""), points: v.curve.map((p) => [p.at, p.cpu] as Point) })),
      resting: [{ name: "挂单", points: w.trader.map((t) => [t.at, t.trader.resting ?? null] as Point), area: true }],
      ticks: [{ name: "tick / 分钟", points: tickRate(w.trader), area: true }],
    }),
    [w],
  );
  const axis = { from: w.from_ms, to: w.to_ms, marks, bands: halts, onPick };

  return (
    <div className="space-y-5">
      {/* One unit per chart: a load of 0.3 drawn against 91 % memory is a flat line. */}
      <div className="grid gap-5 xl:grid-cols-2">
        <ChartCard title="系统负载（1 分钟平均）" icon={<Cpu className="h-4 w-4" />} series={charts.load} axis={axis} />
        <ChartCard title="系统可用内存" icon={<Server className="h-4 w-4" />} series={charts.mem} axis={axis} unit="%" decimals={1} />
        <ChartCard title="资源压力（等待 CPU / 内存 / IO 的时间占比，10 秒平均）" series={charts.psi} axis={axis} unit="%" />
        <ChartCard title="各服务内存（MiB）" series={charts.unitMem} axis={axis} decimals={1} />
        <ChartCard title="各服务 CPU（单核 %）" series={charts.unitCpu} axis={axis} unit="%" decimals={3} />
        <ChartCard title="交易进程挂单数（红底为停机）" icon={<Activity className="h-4 w-4" />} series={charts.resting} axis={axis} decimals={0} />
        <ChartCard title="交易进程收到的行情（tick / 分钟）" series={charts.ticks} axis={axis} decimals={1} note="由累计计数求差；进程重启时计数归零，那一段留空而不是负数。" />
      </div>

      <Card title="各服务资源" icon={<Server className="h-4 w-4" />} bodyClassName="p-0" extra={<span>每 {w.every_s} 秒一个采样</span>}>
        <Table head={["服务", "内存 最小 / 平均 / 最大", "CPU 平均 / p95 / 最大", "停止的采样"]}>
          {units.map(([u, v]) => (
            <tr key={u}>
              <td className="font-mono text-xs text-ink">{u}</td>
              <td className="font-mono text-xs tabular-nums">
                {v.memory ? `${mib(v.memory.min)!.toFixed(1)} / ${mib(v.memory.avg)!.toFixed(1)} / ${mib(v.memory.max)!.toFixed(1)} MiB` : "—"}
              </td>
              <td className="font-mono text-xs tabular-nums">
                {v.cpu_percent ? `${v.cpu_percent.avg.toFixed(3)} / ${v.cpu_percent.p95.toFixed(3)} / ${v.cpu_percent.max.toFixed(3)}%` : "—"}
              </td>
              <td>{v.samples_down ? <Badge tone="warn">{v.samples_down}</Badge> : <span className="text-ink-faint">0</span>}</td>
            </tr>
          ))}
        </Table>
      </Card>

      <Card title="事件" icon={<History className="h-4 w-4" />} bodyClassName="p-0" extra={<span>{events.length} 条 · 点一条打开那一刻</span>}>
        {events.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-ink-faint">这段时间没有状态变化、停机或告警。</p>
        ) : (
          <ul className="max-h-80 divide-y divide-line/60 overflow-auto">
            {[...events].reverse().map((e, k) => (
              <li key={k}>
                <button className="flex w-full items-center gap-3 px-4 py-2 text-left text-sm hover:bg-surface-hover/50" onClick={() => onPick(e.at)}>
                  <span className="shrink-0 font-mono text-xs text-ink-muted">{fmtTime(e.at)}</span>
                  <Badge tone={eventTone(e.what)} dot>
                    {EVENT_NAMES[e.what] ?? e.what}
                  </Badge>
                  <span className="min-w-0 truncate text-xs text-ink-muted">{eventDetail(e)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  );
}

function eventDetail(e: BlackboxEvent) {
  return [e.unit, e.message, e.reason, e.result && `结果 ${e.result}`, e.exit_status && `退出码 ${e.exit_status}`].filter(Boolean).join(" · ");
}

function ChartCard({
  title,
  icon,
  series,
  axis,
  unit,
  decimals,
  note,
}: {
  title: string;
  icon?: ReactNode;
  series: { name: string; points: Point[]; area?: boolean }[];
  axis: { from: number; to: number; marks: { at: number; label?: string; color?: string }[]; bands: [number, number][]; onPick: (t: number) => void };
  unit?: string;
  decimals?: number;
  note?: string;
}) {
  const has = series.some((s) => s.points.some((p) => p[1] !== null));
  return (
    <Card title={title} icon={icon} bodyClassName="px-2 pb-2 pt-3">
      {has ? (
        <TimeSeries
          series={series}
          height={200}
          from={axis.from}
          to={axis.to}
          marks={axis.marks}
          bands={axis.bands}
          onPick={(ms) => axis.onPick(Math.round(ms))}
          unit={unit}
          decimals={decimals}
          zoom
        />
      ) : (
        <p className="px-2 py-10 text-center text-sm text-ink-faint">这段时间没有数据</p>
      )}
      {note && <p className="px-2 pt-1 text-xs text-ink-faint">{note}</p>}
    </Card>
  );
}

/**
 * Ticks per minute from the cumulative counter. The counter restarts with
 * the process, so a drop is a restart and has no rate, not a negative one.
 */
function tickRate(samples: BlackboxWindow["trader"]): Point[] {
  const out: Point[] = [];
  for (let k = 1; k < samples.length; k++) {
    const [a, b] = [samples[k - 1], samples[k]];
    const dt = (b.at - a.at) / 60_000;
    const dn = (b.trader.ticks ?? NaN) - (a.trader.ticks ?? NaN);
    out.push([b.at, dt > 0 && dn >= 0 ? dn / dt : null]);
  }
  return out;
}

/** One moment, opened up in a drawer beside the charts. */
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
    return (journals.data ?? [])
      .map((j) => j.id)
      .filter((id) => stamp(id) <= at)
      .sort((a, b) => stamp(b) - stamp(a))[0];
  }, [journals.data, at]);
  const records = useQuery({
    queryKey: ["records", journalId, "around", at],
    queryFn: () =>
      api.recordsBetween(journalId as string, ["submitted", "outcome", "fill", "cancelled", "refused", "operator", "reconciled"], 300, (at - AROUND_MS) * 1e6, (at + AROUND_MS) * 1e6),
    enabled: Boolean(journalId),
  });
  const logs = useQuery({ queryKey: ["journal-log", "oqp-live", at], queryFn: () => api.journalLog("oqp-live.service", at - AROUND_MS, at + AROUND_MS, 400) });
  const s = snap.data;
  const t = s?.trader?.trader;
  const h = s?.host?.host;
  const gib = (b: number | null | undefined) => (b ? (b / 2 ** 30).toFixed(1) : "—");

  return (
    <Drawer title={`${fmtTime(at)} 这一刻`} onClose={onClose} width="max-w-3xl">
      <div className="space-y-5">
        {snap.isLoading ? (
          <Skeleton rows={4} />
        ) : snap.isError ? (
          <ErrorState error={snap.error} what="那一刻的记录" />
        ) : (
          <div className="grid gap-4 md:grid-cols-2">
            <Card
              title="交易进程"
              icon={<Activity className="h-4 w-4" />}
              className="md:col-span-2"
              tone={t?.halted || t?.journal_lost ? "bad" : undefined}
              extra={s?.trader && <span>记录于 {fmtTime(s.trader.at, false)}</span>}
            >
              {t ? (
                <div className="space-y-3">
                  <div className="flex items-center gap-2 text-sm">
                    <StatusDot tone={t.halted ? "bad" : "good"} />
                    <span className={t.halted ? "text-bad" : "text-ink"}>{t.halted ? `已停机：${t.halt_reason ?? "未说明原因"}` : "交易中"}</span>
                  </div>
                  <KV
                    cols={2}
                    items={[
                      ["持仓", (t.positions ?? []).map((p) => `${p.side === "LONG" ? "多" : p.side === "SHORT" ? "空" : p.side} ${p.amount.replace("-", "")}`).join(" · ") || "无"],
                      ["挂单", String(t.resting ?? "—")],
                      ["累计 tick", String(t.ticks ?? "—")],
                      ["读不出的消息", <span key="u" className={(t.feed?.unreadable ?? 0) > 0 ? "text-bad" : ""}>{t.feed?.unreadable ?? "—"}</span>],
                      [
                        "进程自检",
                        // "Not yet checked" is its own answer, never shown as agreement.
                        <span key="r" className={t.reconcile?.agreed === false ? "text-bad" : t.reconcile?.agreed == null ? "text-warn" : "text-good"}>
                          {t.reconcile?.agreed === true ? "一致" : t.reconcile?.agreed === false ? "不一致" : "尚未核对"}
                        </span>,
                      ],
                      ...(t.pnl
                        ? ([
                            ["本次运行盈亏", t.pnl.net],
                            ["手续费", t.pnl.fees],
                            ["权益", t.pnl.equity],
                          ] as [ReactNode, ReactNode][])
                        : []),
                    ]}
                  />
                  {t.journal_lost && <p className="text-sm text-bad">日志无法写入：{t.journal_lost}</p>}
                </div>
              ) : (
                <p className="text-sm text-warn">那一刻前后 10 分钟没有交易进程的记录（控制口无应答或进程未运行）。</p>
              )}
            </Card>
            <Card title="系统" icon={<Cpu className="h-4 w-4" />}>
              {h ? (
                <KV
                  items={[
                    ["负载", h.load?.map((v) => v.toFixed(2)).join(" / ") ?? "—"],
                    ["可用内存", `${gib(h.mem_available)} / ${gib(h.mem_total)} GiB`],
                    ["压力 CPU", h.psi_cpu == null ? "—" : `${h.psi_cpu}%`],
                    ["压力 内存", h.psi_memory == null ? "—" : `${h.psi_memory}%`],
                    ["压力 IO", h.psi_io == null ? "—" : `${h.psi_io}%`],
                    ["时钟", <span key="c" className={h.clock_synced ? "text-good" : "text-warn"}>{h.clock_synced ? "已同步" : "未同步"}</span>],
                  ]}
                />
              ) : (
                <p className="text-sm text-warn">没有记录。</p>
              )}
            </Card>
            <Card title="服务" icon={<Server className="h-4 w-4" />}>
              {Object.keys(s?.units ?? {}).length === 0 ? (
                <p className="text-sm text-warn">没有记录。</p>
              ) : (
                <ul className="space-y-2 text-sm">
                  {Object.entries(s?.units ?? {}).map(([u, v]) => (
                    <li key={u} className="flex items-center gap-2">
                      <StatusDot tone={v.active ? "good" : "warn"} />
                      <span className="min-w-0 flex-1 truncate font-mono text-xs text-ink">{u.replace(".service", "")}</span>
                      <span className={v.active ? "text-xs text-ink-muted tabular-nums" : "text-xs text-warn"}>
                        {v.active ? `${fmtBytes(v.mem ?? 0)} · ${v.tasks ?? "—"} 任务` : "未运行"}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </Card>
          </div>
        )}

        <Card title="前后 2 分钟的决策与成交" bodyClassName="p-0" extra={<span className="font-mono">{journalId ?? "没有那时的 journal"}</span>}>
          {!journalId ? (
            <p className="px-4 py-6 text-center text-sm text-ink-faint">{journals.isLoading ? "读取 journal 列表…" : "那一刻之前没有开始的 journal。"}</p>
          ) : records.isLoading ? (
            <div className="p-4">
              <Skeleton rows={3} />
            </div>
          ) : records.isError ? (
            <div className="p-4">
              <ErrorState error={records.error} what="交易日志" />
            </div>
          ) : (
            <RecordRows page={records.data!} empty="这 4 分钟里交易进程没有下单、成交或撤单。" />
          )}
        </Card>

        <Card title="前后 2 分钟交易进程的输出" icon={<Terminal className="h-4 w-4" />} bodyClassName="p-0" extra={<span>oqp-live.service · systemd journal</span>}>
          {logs.isLoading ? (
            <div className="p-4">
              <Skeleton rows={3} />
            </div>
          ) : logs.isError ? (
            <div className="p-4">
              <ErrorState error={logs.error} what="程序输出" />
            </div>
          ) : logs.data!.lines.length === 0 ? (
            <p className="px-4 py-6 text-center text-sm text-ink-faint">这 4 分钟没有输出；输出改写进 systemd journal 之前的时段只有不带时间的日志文件，在「日志」页查看。</p>
          ) : (
            // Text, never markup: the trader's output carries venue strings.
            <pre className="max-h-96 overflow-auto rounded-b-[var(--radius-card)] bg-[#07090c] p-3 font-mono text-[12px] leading-5 text-ink">{logs.data!.lines.join("\n")}</pre>
          )}
        </Card>
      </div>
    </Drawer>
  );
}
