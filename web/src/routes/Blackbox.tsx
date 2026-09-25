import { useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Activity, Cpu, History, RotateCcw, Server, Terminal } from "lucide-react";

import { api, type BlackboxEvent, type BlackboxWindow } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { type Point, TimeSeries } from "@/ui/charts";
import { useThemeColors } from "@/ui/theme";
import { pair, said, tr } from "@/i18n";
import { Badge, Button, Card, Drawer, Freshness, KV, PageHeader, Segmented, StatusDot, Table, fmtBytes, fmtTime, type Tone } from "@/ui/kit";

import { RecordRows } from "./Trading";

const presets = (): { value: number; label: string }[] => [
  { value: 1, label: tr("1 小时", "1 h") },
  { value: 6, label: tr("6 小时", "6 h") },
  { value: 24, label: tr("24 小时", "24 h") },
  { value: 72, label: tr("3 天", "3 d") },
  { value: 168, label: tr("7 天", "7 d") },
];

const eventNames = (): Record<string, string> => ({
  unit_started: tr("服务启动", "Service started"),
  unit_stopped: tr("服务停止", "Service stopped"),
  trader_halted: tr("交易进程停机", "Trader halted"),
  trader_resumed: tr("交易进程解除停机", "Trader resumed"),
  control_lost: tr("控制口无应答", "Control port not answering"),
  control_back: tr("控制口恢复", "Control port back"),
  alert_raised: tr("告警触发", "Alert raised"),
  alert_cleared: tr("告警恢复", "Alert cleared"),
  recording_started: tr("主机代理启动，开始记录（此前的空白没有记录）", "Host agent started recording (nothing was recorded in the gap before)"),
});

/** An event's display name; unknown kinds show as they are. */
function eventName(what: string): string {
  return eventNames()[what] ?? what;
}

/** An event's state colour: trouble starting is amber, trouble ending is green, the rest neutral. */
function eventTone(what: string): Tone {
  if (/stopped|halted|lost|raised/.test(what)) return "warn";
  if (/started|resumed|back|cleared/.test(what) && what !== "recording_started") return "good";
  return "neutral";
}


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
        title={tr("黑匣子复盘", "Black box review")}
        description={tr(
          "主机代理每 30 秒记录一次系统、各服务和交易进程的状态，保留 90 天；状态变化和告警记为事件。点曲线上的点或事件，打开那一刻。",
          "The host agent records the state of the system, each service and the trader every 30 seconds and keeps it for 90 days; state changes and alerts are recorded as events. Click a point on a chart or an event to open that moment.",
        )}
        meta={end === null && <Freshness at={q.dataUpdatedAt} fetching={q.isFetching} staleAfterS={120} onRefresh={() => q.refetch()} />}
        actions={
          <>
            <Segmented value={hours} options={presets()} onChange={setHours} />
            <label className="flex items-center gap-1.5 text-xs text-ink-muted">
              {tr("截止", "Until")}
              <input
                type="datetime-local"
                className="h-8 rounded-md border border-line-strong bg-surface-raised px-2 text-xs text-ink outline-none focus:border-accent"
                value={toLocalInput(to)}
                onChange={(e) => setEnd(e.target.value ? new Date(e.target.value).getTime() : null)}
              />
            </label>
            {end !== null && (
              <Button size="sm" variant="ghost" icon={<RotateCcw className="h-3.5 w-3.5" />} onClick={() => setEnd(null)}>
                {tr("回到现在", "Back to now")}
              </Button>
            )}
          </>
        }
      />

      {q.isLoading ? (
        <Skeleton rows={10} />
      ) : q.isError ? (
        <ErrorState error={q.error} what={tr("黑匣子记录", "black box records")} />
      ) : !q.data || Object.keys(q.data.units).length === 0 ? (
        <Empty
          title={tr("这段时间没有记录。", "Nothing recorded in this window.")}
          next={tr(
            "黑匣子从主机代理这次部署开始记录；更早的时间没有数据。换一个时间范围，或点「回到现在」。",
            "The black box records from the host agent's current deployment on; there is no data before that. Pick another window, or click \"Back to now\".",
          )}
        />
      ) : (
        <Window w={q.data} onPick={setMoment} moment={moment} />
      )}

      {moment !== null && <Moment at={moment} onClose={() => setMoment(null)} />}
    </div>
  );
}

function Window({ w, onPick, moment }: { w: BlackboxWindow; onPick: (t: number) => void; moment: number | null }) {
  // Drawn on a canvas, so the colours come from the theme rather than CSS.
  const tc = useThemeColors();
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
      ...events.map((e) => ({ at: e.at, label: eventName(e.what), color: eventTone(e.what) === "warn" ? tc.warn : tc.faint })),
      ...(moment !== null ? [{ at: moment, label: tr("打开的时刻", "Opened moment"), color: tc.accent }] : []),
    ],
    [events, moment, tc],
  );
  const mib = (b: number | null | undefined) => (b == null ? null : b / 1_048_576);

  const charts = useMemo(
    () => ({
      load: [{ name: tr("负载", "Load"), points: w.host.map((h) => [h.at, h.host.load?.[0] ?? null] as Point), area: true }],
      mem: [
        {
          name: tr("可用内存", "Available memory"),
          points: w.host.map((h) => [h.at, h.host.mem_total ? ((h.host.mem_available ?? 0) / h.host.mem_total) * 100 : null] as Point),
          area: true,
        },
      ],
      psi: [
        { name: "CPU", points: w.host.map((h) => [h.at, h.host.psi_cpu ?? null] as Point) },
        { name: tr("内存", "Memory"), points: w.host.map((h) => [h.at, h.host.psi_memory ?? null] as Point) },
        { name: "IO", points: w.host.map((h) => [h.at, h.host.psi_io ?? null] as Point) },
      ],
      unitMem: Object.entries(w.units).map(([u, v]) => ({ name: u.replace(".service", ""), points: v.curve.map((p) => [p.at, mib(p.mem)] as Point) })),
      unitCpu: Object.entries(w.units).map(([u, v]) => ({ name: u.replace(".service", ""), points: v.curve.map((p) => [p.at, p.cpu] as Point) })),
      resting: [{ name: tr("挂单", "Open orders"), points: w.trader.map((t) => [t.at, t.trader.resting ?? null] as Point), area: true }],
      ticks: [{ name: tr("tick / 分钟", "ticks / min"), points: tickRate(w.trader), area: true }],
    }),
    [w],
  );
  const axis = { from: w.from_ms, to: w.to_ms, marks, bands: halts, onPick };

  return (
    <div className="space-y-5">
      {/* One unit per chart: a load of 0.3 drawn against 91 % memory is a flat line. */}
      <div className="grid gap-5 xl:grid-cols-2">
        <ChartCard title={tr("系统负载（1 分钟平均）", "System load (1-minute average)")} icon={<Cpu className="h-4 w-4" />} series={charts.load} axis={axis} />
        <ChartCard title={tr("系统可用内存", "System available memory")} icon={<Server className="h-4 w-4" />} series={charts.mem} axis={axis} unit="%" decimals={1} />
        <ChartCard title={tr("资源压力（等待 CPU / 内存 / IO 的时间占比，10 秒平均）", "Resource pressure (share of time waiting on CPU / memory / IO, 10 s average)")} series={charts.psi} axis={axis} unit="%" />
        <ChartCard title={tr("各服务内存（MiB）", "Memory per service (MiB)")} series={charts.unitMem} axis={axis} decimals={1} />
        <ChartCard title={tr("各服务 CPU（单核 %）", "CPU per service (% of one core)")} series={charts.unitCpu} axis={axis} unit="%" decimals={3} />
        <ChartCard title={tr("交易进程挂单数（红底为停机）", "Trader open orders (red background = halted)")} icon={<Activity className="h-4 w-4" />} series={charts.resting} axis={axis} decimals={0} />
        <ChartCard title={tr("交易进程收到的行情（tick / 分钟）", "Market data received by the trader (ticks / min)")} series={charts.ticks} axis={axis} decimals={1} note={tr(
            "由累计计数求差；进程重启时计数归零，那一段留空而不是负数。",
            "Differenced from a cumulative counter; the counter resets when the process restarts, and that stretch is left blank rather than negative.",
          )} />
      </div>

      <Card
        title={tr("各服务资源", "Resources per service")}
        icon={<Server className="h-4 w-4" />}
        bodyClassName="p-0"
        extra={<span>{tr(`每 ${w.every_s} 秒一个采样`, `One sample every ${w.every_s} s`)}</span>}
      >
        <Table head={[tr("服务", "Service"), tr("内存 最小 / 平均 / 最大", "Memory min / avg / max"), tr("CPU 平均 / p95 / 最大", "CPU avg / p95 / max"), tr("停止的采样", "Samples down")]}>
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

      <Card
        title={tr("事件", "Events")}
        icon={<History className="h-4 w-4" />}
        bodyClassName="p-0"
        extra={
          <span>
            {tr(`${events.length} 条 · 点一条打开那一刻`, `${events.length} event${events.length === 1 ? "" : "s"} · click one to open that moment`)}
          </span>
        }
      >
        {events.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-ink-faint">{tr("这段时间没有状态变化、停机或告警。", "No state changes, halts or alerts in this window.")}</p>
        ) : (
          <ul className="max-h-80 divide-y divide-line/60 overflow-auto">
            {[...events].reverse().map((e, k) => (
              <li key={k}>
                <button className="flex w-full items-center gap-3 px-4 py-2 text-left text-sm hover:bg-surface-hover/50" onClick={() => onPick(e.at)}>
                  <span className="shrink-0 font-mono text-xs text-ink-muted">{fmtTime(e.at)}</span>
                  <Badge tone={eventTone(e.what)} dot>
                    {eventName(e.what)}
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
  // The reason is the agent's when it says why the port went quiet, and
  // the trader's own words when it says why it halted; a pair is read in
  // the reader's language, a lone rendering is shown as it is.
  return [e.unit, said(e), pair(e.reason, e.reason_en), e.result && tr(`结果 ${e.result}`, `result ${e.result}`), e.exit_status && tr(`退出码 ${e.exit_status}`, `exit status ${e.exit_status}`)].filter(Boolean).join(" · ");
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
        <p className="px-2 py-10 text-center text-sm text-ink-faint">{tr("这段时间没有数据", "No data in this window")}</p>
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
  // The trader's unit as the agent names it; no guess at a deployment's naming.
  const host = useQuery({ queryKey: ["ops", "host"], queryFn: api.host, staleTime: 60_000 });
  const unit = host.data?.trader_unit;
  const logs = useQuery({
    queryKey: ["journal-log", unit, at],
    queryFn: () => api.journalLog(unit as string, at - AROUND_MS, at + AROUND_MS, 400),
    enabled: Boolean(unit),
  });
  const s = snap.data;
  const t = s?.trader?.trader;
  const h = s?.host?.host;
  const gib = (b: number | null | undefined) => (b ? (b / 2 ** 30).toFixed(1) : "—");

  return (
    <Drawer title={tr(`${fmtTime(at)} 这一刻`, `The moment at ${fmtTime(at)}`)} onClose={onClose} width="max-w-3xl">
      <div className="space-y-5">
        {snap.isLoading ? (
          <Skeleton rows={4} />
        ) : snap.isError ? (
          <ErrorState error={snap.error} what={tr("那一刻的记录", "the record at that moment")} />
        ) : (
          <div className="grid gap-4 md:grid-cols-2">
            <Card
              title={tr("交易进程", "Trader")}
              icon={<Activity className="h-4 w-4" />}
              className="md:col-span-2"
              tone={t?.halted || t?.journal_lost ? "bad" : undefined}
              extra={s?.trader && <span>{tr(`记录于 ${fmtTime(s.trader.at, false)}`, `Recorded at ${fmtTime(s.trader.at, false)}`)}</span>}
            >
              {t ? (
                <div className="space-y-3">
                  <div className="flex items-center gap-2 text-sm">
                    <StatusDot tone={t.halted ? "bad" : "good"} />
                    <span className={t.halted ? "text-bad" : "text-ink"}>{t.halted
                        ? t.halt_reason
                          ? tr(`已停机：${t.halt_reason}`, `Halted: ${t.halt_reason}`)
                          : tr("已停机：未说明原因", "Halted: no reason given")
                        : tr("交易中", "Trading")}</span>
                  </div>
                  <KV
                    cols={2}
                    items={[
                      [
                        tr("持仓", "Positions"),
                        (t.positions ?? [])
                          .map((p) => `${p.side === "LONG" ? tr("多", "Long") : p.side === "SHORT" ? tr("空", "Short") : p.side} ${p.amount.replace("-", "")}`)
                          .join(" · ") || tr("无", "None"),
                      ],
                      [tr("挂单", "Open orders"), String(t.resting ?? "—")],
                      [tr("累计 tick", "Total ticks"), String(t.ticks ?? "—")],
                      [tr("读不出的消息", "Unreadable messages"), <span key="u" className={(t.feed?.unreadable ?? 0) > 0 ? "text-bad" : ""}>{t.feed?.unreadable ?? "—"}</span>],
                      [
                        tr("进程自检", "Self-check"),
                        // "Not yet checked" is its own answer, never shown as agreement.
                        <span key="r" className={t.reconcile?.agreed === false ? "text-bad" : t.reconcile?.agreed == null ? "text-warn" : "text-good"}>
                          {t.reconcile?.agreed === true ? tr("一致", "Agrees") : t.reconcile?.agreed === false ? tr("不一致", "Disagrees") : tr("尚未核对", "Not yet checked")}
                        </span>,
                      ],
                      ...(t.pnl
                        ? ([
                            [tr("本次运行盈亏", "Run P&L"), t.pnl.net ?? tr("未测得", "not measured")],
                            [tr("手续费", "Fees"), t.pnl.fees ?? tr("未测得", "not measured")],
                            [tr("权益", "Equity"), t.pnl.equity],
                          ] as [ReactNode, ReactNode][])
                        : []),
                    ]}
                  />
                  {t.journal_lost && <p className="text-sm text-bad">{tr(`日志无法写入：${t.journal_lost}`, `Journal cannot be written: ${t.journal_lost}`)}</p>}
                </div>
              ) : (
                <p className="text-sm text-warn">
                  {tr(
                    "那一刻前后 10 分钟没有交易进程的记录（控制口无应答或进程未运行）。",
                    "No trader record within 10 minutes of that moment (the control port did not answer or the process was not running).",
                  )}
                </p>
              )}
            </Card>
            <Card title={tr("系统", "System")} icon={<Cpu className="h-4 w-4" />}>
              {h ? (
                <KV
                  items={[
                    [tr("负载", "Load"), h.load?.map((v) => v.toFixed(2)).join(" / ") ?? "—"],
                    [tr("可用内存", "Available memory"), `${gib(h.mem_available)} / ${gib(h.mem_total)} GiB`],
                    [tr("压力 CPU", "Pressure CPU"), h.psi_cpu == null ? "—" : `${h.psi_cpu}%`],
                    [tr("压力 内存", "Pressure memory"), h.psi_memory == null ? "—" : `${h.psi_memory}%`],
                    [tr("压力 IO", "Pressure IO"), h.psi_io == null ? "—" : `${h.psi_io}%`],
                    [tr("时钟", "Clock"), <span key="c" className={h.clock_synced ? "text-good" : "text-warn"}>{h.clock_synced ? tr("已同步", "Synced") : tr("未同步", "Not synced")}</span>],
                  ]}
                />
              ) : (
                <p className="text-sm text-warn">{tr("没有记录。", "Nothing recorded.")}</p>
              )}
            </Card>
            <Card title={tr("服务", "Services")} icon={<Server className="h-4 w-4" />}>
              {Object.keys(s?.units ?? {}).length === 0 ? (
                <p className="text-sm text-warn">{tr("没有记录。", "Nothing recorded.")}</p>
              ) : (
                <ul className="space-y-2 text-sm">
                  {Object.entries(s?.units ?? {}).map(([u, v]) => (
                    <li key={u} className="flex items-center gap-2">
                      <StatusDot tone={v.active ? "good" : "warn"} />
                      <span className="min-w-0 flex-1 truncate font-mono text-xs text-ink">{u.replace(".service", "")}</span>
                      <span className={v.active ? "text-xs text-ink-muted tabular-nums" : "text-xs text-warn"}>
                        {v.active
                          ? tr(`${fmtBytes(v.mem ?? 0)} · ${v.tasks ?? "—"} 任务`, `${fmtBytes(v.mem ?? 0)} · ${v.tasks ?? "—"} tasks`)
                          : tr("未运行", "Not running")}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </Card>
          </div>
        )}

        <Card
          title={tr("前后 2 分钟的决策与成交", "Decisions and fills within 2 minutes")}
          bodyClassName="p-0"
          extra={<span className="font-mono">{journalId ?? tr("没有那时的 journal", "No journal from then")}</span>}
        >
          {!journalId ? (
            <p className="px-4 py-6 text-center text-sm text-ink-faint">{journals.isLoading ? tr("读取 journal 列表…", "Reading the journal list…") : tr("那一刻之前没有开始的 journal。", "No journal started before that moment.")}</p>
          ) : records.isLoading ? (
            <div className="p-4">
              <Skeleton rows={3} />
            </div>
          ) : records.isError ? (
            <div className="p-4">
              <ErrorState error={records.error} what={tr("交易日志", "journal")} />
            </div>
          ) : (
            <RecordRows page={records.data!} empty={tr("这 4 分钟里交易进程没有下单、成交或撤单。", "The trader placed, filled or cancelled no orders in these 4 minutes.")} />
          )}
        </Card>

        <Card
          title={tr("前后 2 分钟交易进程的输出", "Trader output within 2 minutes")}
          icon={<Terminal className="h-4 w-4" />}
          bodyClassName="p-0"
          extra={<span>{unit ?? tr("交易进程", "trader")} · systemd journal</span>}
        >
          {!unit ? (
            <p className="px-4 py-6 text-center text-sm text-ink-faint">{host.isPending
                ? tr("读取中…", "Loading…")
                : tr("主机代理没有报告交易进程是哪个服务，无法取它的输出。", "The host agent did not report which service is the trader, so its output cannot be read.")}</p>
          ) : logs.isLoading ? (
            <div className="p-4">
              <Skeleton rows={3} />
            </div>
          ) : logs.isError ? (
            <div className="p-4">
              <ErrorState error={logs.error} what={tr("程序输出", "program output")} />
            </div>
          ) : logs.data!.lines.length === 0 ? (
            <p className="px-4 py-6 text-center text-sm text-ink-faint">
              {tr(
                "这 4 分钟没有输出；输出改写进 systemd journal 之前的时段只有不带时间的日志文件，在「日志」页查看。",
                "No output in these 4 minutes. Before output went to the systemd journal there are only untimestamped log files; see the Logs page.",
              )}
            </p>
          ) : (
            // Text, never markup: the trader's output carries venue strings.
            <pre className="max-h-96 overflow-auto rounded-b-[var(--radius-card)] bg-term p-3 font-mono text-[12px] leading-5 text-term-ink">{logs.data!.lines.join("\n")}</pre>
          )}
        </Card>
      </div>
    </Drawer>
  );
}
