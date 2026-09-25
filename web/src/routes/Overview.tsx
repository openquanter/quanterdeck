import { Suspense, lazy, useMemo, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  Activity as ActivityIcon,
  AlertTriangle,
  ArrowDownRight,
  ArrowLeftRight,
  ArrowRight,
  ArrowUpRight,
  Bell,
  BookCheck,
  CheckCircle2,
  Clock,
  Cpu,
  FileCheck2,
  GitCompareArrows,
  HardDrive,
  Layers,
  LineChart,
  ListOrdered,
  MemoryStick,
  Radio,
  Scale,
  Server,
  ShieldAlert,
  TrendingUp,
  UserCog,
  Wallet,
} from "lucide-react";

import { api, type BlackboxWindow, type TraderStatus } from "@/api/client";
import { ErrorState } from "@/components/States";
import { kindNames, TraderActions, lotsText, recordText, useCaps, useNewestJournal, useTrader } from "@/features/trading";
import { intlLocale, pair, said, tr } from "@/i18n";
import { Ago, Badge, Card, Freshness, IconTile, Money, PageHeader, Ring, Stat, StatusDot, agoText, cx, fmtDuration, type Hue, type Tone } from "@/ui/kit";

// The chart library is most of the bundle; the overview's numbers should
// not wait for it.
const TimeSeries = lazy(() => import("@/ui/charts").then((m) => ({ default: m.TimeSeries })));
const Sparkline = lazy(() => import("@/ui/charts").then((m) => ({ default: m.Sparkline })));

const chartFallback = (h: number) => <div className="animate-pulse rounded-xl bg-surface-raised" style={{ height: h }} />;

/** The last day of the black box, shared by every chart on this page. */
export function useDay() {
  const to = Math.floor(Date.now() / 60_000) * 60_000;
  const from = to - 24 * 3_600_000;
  const q = useQuery({ queryKey: ["blackbox", "overview", to], queryFn: () => api.blackbox(from, to, 240), refetchInterval: 60_000, placeholderData: (p) => p });
  return { ...q, from, to };
}

/**
 * The first screen answers one question — is everything all right — and
 * every "no" links to where it is dealt with (docs/UI-V4 §4.2). Around
 * it, what the run is doing as it happens: the price and the orders
 * resting against it, what the run has made, and what the host is using.
 */
export function Overview() {
  const caps = useCaps();
  const ops = caps.data?.ops?.available === true;
  const writable = caps.data?.writes.available === true;
  const status = useTrader(ops);
  const day = useDay();

  if (caps.data && !ops) return <NoHost />;

  return (
    <div className="space-y-6">
      <PageHeader
        title={tr("总览", "Overview")}
        description={tr("交易主机此刻是否一切正常；任何一项不正常都可以点进去处理。", "Whether everything on the trading host is all right now; click any item that is not to deal with it.")}
        meta={<Freshness at={status.dataUpdatedAt} fetching={status.isFetching} staleAfterS={30} onRefresh={() => status.refetch()} />}
      />
      <Banner status={status.data} failed={status.isError} writable={writable} />
      {status.data && <Kpis s={status.data} day={day.data} />}
      <div className="grid gap-6 xl:grid-cols-5">
        <div className="xl:col-span-3">
          <PriceAndOrders s={status.data} />
        </div>
        <div className="xl:col-span-2">
          <Health s={status.data} failed={status.isError} />
        </div>
      </div>
      <div className="grid gap-6 xl:grid-cols-5">
        <div className="xl:col-span-3">
          <PnlChart day={day.data} from={day.from} to={day.to} />
        </div>
        <div className="xl:col-span-2">
          <Alerts />
        </div>
      </div>
      <div className="grid gap-6 xl:grid-cols-5">
        <div className="xl:col-span-3">
          <Resources day={day.data} failed={day.isError} error={day.error} from={day.from} to={day.to} />
        </div>
        <div className="xl:col-span-2">
          <Activity />
        </div>
      </div>
    </div>
  );
}

function Banner({ status: s, failed, writable }: { status: TraderStatus | undefined; failed: boolean; writable: boolean }) {
  const tone: Tone = failed || s?.halted ? "bad" : s ? "good" : "neutral";
  const lastTickAge = s?.last_tick ? (s.now_ns - s.last_tick.local_ns) / 1e9 : null;
  const up = s?.pnl ? (Date.now() - s.pnl.since_ms) / 1000 : null;
  const last = s?.last_tick && s.price_scale !== undefined ? s.last_tick.last / 10 ** s.price_scale : null;
  return (
    <div
      className={cx(
        "relative flex flex-wrap items-center gap-5 overflow-hidden rounded-[var(--radius-card)] border px-6 py-5 shadow-[var(--shadow-card)]",
        tone === "bad" ? "border-bad/30 bg-bad/6" : "border-line bg-surface",
      )}
    >
      {tone !== "bad" && (
        // A strip of the four colours along the top edge: live, and fine.
        <div className="pointer-events-none absolute inset-x-0 top-0 h-1 bg-gradient-to-r from-hue-blue via-hue-green to-hue-yellow" />
      )}
      <div className={cx("relative flex h-12 w-12 items-center justify-center rounded-2xl", tone === "bad" ? "bg-bad/12 text-bad" : "bg-good/12 text-good")}>
        {tone === "bad" ? <AlertTriangle className="h-6 w-6" /> : <Radio className="h-6 w-6" />}
        {tone === "good" && <span className="absolute right-1 top-1 h-2.5 w-2.5 animate-ping rounded-full bg-good/60" />}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2 text-xl font-medium text-ink">
          {failed
            ? tr("交易进程无应答", "Trader not answering")
            : !s
              ? tr("读取中…", "Loading…")
              : s.halted
                ? tr("交易进程已停机", "Trader halted")
                : tr("交易进程运行中", "Trader running")}
          {s && <Badge tone={s.deployment === "Live" ? "bad" : "accent"}>{s.deployment === "Live" ? tr("主网", "Mainnet") : tr("测试网", "Testnet")}</Badge>}
        </div>
        <div className="mt-1 text-sm text-ink-muted">
          {failed
            ? tr(
                "控制口没有回应：进程可能没在运行，或卡住了。去「主机与服务」看服务状态和日志。",
                "The control port does not answer: the process may not be running, or it is stuck. See service status and logs under \"Host & services\".",
              )
            : s?.halted
              ? s.halt_reason
                ? tr(`原因：${s.halt_reason}`, `Reason: ${s.halt_reason}`)
                : tr("原因：未说明", "Reason: not given")
              : s
                ? [
                    `${s.strategy} · ${s.symbol}`,
                    up !== null && tr(`已运行 ${fmtDuration(up)}`, `Running for ${fmtDuration(up)}`),
                    lastTickAge !== null && tr(`最近行情 ${agoText(lastTickAge)}`, `Last tick ${agoText(lastTickAge)}`),
                  ]
                    .filter(Boolean)
                    .join(" · ")
                : " "}
        </div>
      </div>
      {last !== null && (
        <div className="text-right">
          <div className="text-xs text-ink-faint">{tr(`${s?.symbol} 最新价`, `${s?.symbol} last price`)}</div>
          <div key={last} className="oq-flash rounded-lg px-1 font-mono text-2xl font-medium text-ink">
            {last.toLocaleString(intlLocale(), { minimumFractionDigits: s?.price_scale ?? 0 })}
          </div>
        </div>
      )}
      <div className="flex gap-2">
        <TraderActions s={s} writable={writable} compact />
        <Link
          to={failed ? "/host" : "/live"}
          className="inline-flex h-9 items-center gap-1.5 rounded-full bg-accent px-4 text-sm font-medium text-white shadow-sm hover:bg-accent/90"
        >
          {failed ? tr("主机与服务", "Host & services") : tr("查看实盘", "View live")} <ArrowRight className="h-4 w-4" />
        </Link>
      </div>
    </div>
  );
}

/**
 * The fee in a one-line summary.
 *
 * A run whose books were never told what the venue charges reports no
 * fee at all, and that is a different statement from a fee of zero —
 * the console's own rule is that a measured zero and an unavailable
 * figure are opposite facts.
 */
function feesLabel(fees: string | null): string {
  return fees == null ? tr("手续费未测得", "fees not measured") : tr(`手续费 ${fees}`, `fees ${fees}`);
}

/** Trader samples from the black box as points for one field. */
function traderPoints(day: BlackboxWindow | undefined, pick: (t: BlackboxWindow["trader"][number]["trader"]) => number | null | undefined) {
  return (day?.trader ?? []).map((t) => [t.at, pick(t.trader) ?? null] as [number, number | null]).filter((p) => p[1] !== null);
}

export function Kpis({ s, day }: { s: TraderStatus; day?: BlackboxWindow }) {
  const long = s.positions.filter((p) => Number(p.amount) > 0);
  const short = s.positions.filter((p) => Number(p.amount) < 0);
  const pnl = traderPoints(day, (t) => (t.pnl?.net == null ? null : Number(t.pnl.net)));
  const equity = traderPoints(day, (t) => (t.pnl ? Number(t.pnl.equity) : null));
  const resting = traderPoints(day, (t) => t.resting);
  const spark = (points: [number, number | null][]) =>
    points.length > 2 ? (
      <Suspense fallback={chartFallback(36)}>
        <Sparkline points={points} />
      </Suspense>
    ) : (
      <div className="h-9" />
    );
  // Not `?? 0`: an unmeasured net is not a break-even run, and the
  // arrow and the colour are a verdict about direction.
  const net = s.pnl?.net == null ? null : Number(s.pnl.net);
  return (
    <div className="grid gap-5 sm:grid-cols-2 xl:grid-cols-4">
      <Stat
        label={tr("本次运行盈亏", "Run P&L")}
        icon={net == null ? <ActivityIcon /> : net >= 0 ? <TrendingUp /> : <ArrowDownRight />}
        hue={net == null ? "blue" : net >= 0 ? "green" : "red"}
        value={s.pnl ? <Money value={s.pnl.net} signed /> : "—"}
        sub={
          s.pnl
            ? tr(
                `已实现 ${s.pnl.realized} · ${feesLabel(s.pnl.fees)} · 资金费 ${s.pnl.funding}`,
                `Realized ${s.pnl.realized} · ${feesLabel(s.pnl.fees)} · funding ${s.pnl.funding}`,
              )
            : tr("此版本的交易进程不报告盈亏", "This trader version does not report P&L")
        }
        help="run_pnl"
        trend={spark(pnl)}
      />
      <Stat
        label={tr("权益", "Equity")}
        icon={<Wallet />}
        hue="blue"
        value={s.pnl ? <Money value={s.pnl.equity} /> : "—"}
        sub={tr("按最近标记价", "At the latest mark price")}
        trend={spark(equity)}
      />
      <Stat
        label={tr("持仓", "Position")}
        icon={<Scale />}
        hue="purple"
        value={
          s.positions.length === 0 ? (
            tr("空仓", "Flat")
          ) : (
            <span className="flex items-baseline gap-3">
              {long.length > 0 && (
                <span className="inline-flex items-baseline gap-1">
                  <ArrowUpRight className="h-4 w-4 self-center text-ink-faint" />
                  {long.map((p) => p.amount).join(" ")}
                </span>
              )}
              {short.length > 0 && (
                <span className="inline-flex items-baseline gap-1">
                  <ArrowDownRight className="h-4 w-4 self-center text-ink-faint" />
                  {short.map((p) => p.amount.replace("-", "")).join(" ")}
                </span>
              )}
            </span>
          )
        }
        sub={`${s.symbol} · ${
          long.length && short.length
            ? tr("双向持仓：多 / 空", "Hedged: long / short")
            : long.length
              ? tr("多头", "Long")
              : short.length
                ? tr("空头", "Short")
                : tr("无", "None")
        }`}
        help="hedged"
        trend={<PositionBar s={s} />}
      />
      <Stat
        label={tr("挂单", "Open orders")}
        icon={<ListOrdered />}
        hue="orange"
        value={String(s.resting)}
        sub={
          s.limits
            ? tr(
                `上限 ${s.limits.max_working} · 持仓上限 ${lotsText(s.limits.max_position_qty, s.qty_scale)}`,
                `Limit ${s.limits.max_working} · position limit ${lotsText(s.limits.max_position_qty, s.qty_scale)}`,
              )
            : undefined
        }
        trend={spark(resting)}
      />
    </div>
  );
}

/** Each leg against the position limit: how much room is left. */
function PositionBar({ s }: { s: TraderStatus }) {
  const cap = s.limits && s.qty_scale !== undefined ? s.limits.max_position_qty / 10 ** s.qty_scale : null;
  const legs = s.positions.map((p) => ({ side: p.side, qty: Math.abs(Number(p.amount)) }));
  if (!cap) return <div className="h-9" />;
  return (
    <div className="space-y-1.5 px-1 pt-1">
      {legs.map((l) => (
        <div key={l.side} className="flex items-center gap-2 text-[11px] text-ink-faint">
          <span className="w-6">{l.side === "LONG" ? tr("多", "L") : l.side === "SHORT" ? tr("空", "S") : l.side}</span>
          <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-raised">
            <div className="h-full rounded-full bg-hue-purple" style={{ width: `${Math.min(100, (l.qty / cap) * 100)}%` }} />
          </div>
          <span className="w-10 text-right">{Math.round((l.qty / cap) * 100)}%</span>
        </div>
      ))}
    </div>
  );
}

/** The price as the trader sees it, and its resting orders against it. */
export function PriceAndOrders({ s }: { s: TraderStatus | undefined }) {
  const journal = useNewestJournal();
  const ticks = useQuery({
    queryKey: ["records", journal.id, "tick", 360],
    queryFn: () => api.records(journal.id as string, ["tick"], 360),
    enabled: Boolean(journal.id),
    refetchInterval: 5_000,
    placeholderData: (p) => p,
  });
  const orders = useQuery({ queryKey: ["ops", "orders"], queryFn: api.orders, refetchInterval: 10_000 });
  const ps = ticks.data?.price_scale ?? s?.price_scale ?? 0;
  const points = useMemo(
    () => [...(ticks.data?.records ?? [])].reverse().map((r) => [Number(r.fields.seen) / 1e6, Number(r.fields.last) / 10 ** ps] as [number, number]),
    [ticks.data, ps],
  );
  const last = points.at(-1)?.[1];
  const levels = (orders.data?.orders ?? [])
    .filter((o) => o.price_ticks !== null)
    .map((o) => ({ value: (o.price_ticks as number) / 10 ** ps, label: o.side === "BUY" ? (o.closing ? tr("买 平", "Buy close") : tr("买", "Buy")) : o.closing ? tr("卖 平", "Sell close") : tr("卖", "Sell") }));
  // Only the orders near the market are drawn: a ladder a thousand ticks
  // away would flatten the price into a line.
  const near = last === undefined ? [] : levels.filter((l) => Math.abs(l.value - last) <= last * 0.01);
  const nearest = last !== undefined && levels.length ? Math.min(...levels.map((l) => Math.abs(l.value - last))) : null;
  return (
    <Card
      title={tr("价格与挂单", "Price and open orders")}
      icon={<LineChart />}
      hue="blue"
      className="h-full"
      extra={
        <span className="inline-flex items-center gap-1.5">
          <StatusDot tone="good" pulse /> {tr("实时 · 5 秒", "Live · 5 s")}
        </span>
      }
    >
      {points.length < 2 ? (
        chartFallback(280)
      ) : (
        <Suspense fallback={chartFallback(280)}>
          <TimeSeries height={280} decimals={ps} step series={[{ name: tr("最新价", "Last price"), points, area: true }]} levels={near} />
        </Suspense>
      )}
      <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-ink-muted">
        <span>{tr(`挂单 ${levels.length} 张`, `${levels.length} open order${levels.length === 1 ? "" : "s"}`)}</span>
        <span>{tr(`市价 ±1% 以内 ${near.length} 张（虚线）`, `${near.length} within ±1% of the market (dashed)`)}</span>
        {nearest !== null && <span>{tr(`最近一张距市价 ${nearest.toFixed(ps)}`, `Nearest is ${nearest.toFixed(ps)} from the market`)}</span>}
        <Link to="/live?tab=positions" className="ml-auto text-accent hover:underline">
          {tr("全部挂单 →", "All open orders →")}
        </Link>
      </div>
    </Card>
  );
}

type Check = { name: string; icon: ReactNode; hue: Hue; tone: Tone; verdict: string; detail?: ReactNode; to: string };

function Health({ s, failed }: { s: TraderStatus | undefined; failed: boolean }) {
  const live = useQuery({ queryKey: ["live", "latest"], queryFn: api.liveLatest, refetchInterval: 30_000, retry: false });
  const units = useQuery({ queryKey: ["ops", "units"], queryFn: api.units, refetchInterval: 30_000 });
  const host = useQuery({ queryKey: ["ops", "host"], queryFn: api.host, refetchInterval: 60_000 });

  const checks: Check[] = [];
  if (failed) {
    checks.push({
      name: tr("交易进程", "Trader"),
      icon: <ActivityIcon />,
      hue: "red",
      tone: "bad",
      verdict: tr("无应答", "Not answering"),
      detail: tr("控制口没有回应", "The control port does not answer"),
      to: "/host",
    });
  } else if (s) {
    const age = s.last_tick ? (s.now_ns - s.last_tick.local_ns) / 1e9 : null;
    checks.push({
      name: tr("行情", "Market data"),
      icon: <Radio />,
      hue: "blue",
      tone: age === null ? "warn" : age > 120 || s.feed.unreadable > 0 ? "bad" : "good",
      verdict: age === null ? tr("尚无行情", "No data yet") : age > 120 ? tr("行情中断", "Feed stalled") : tr("正常", "OK"),
      detail: `${age === null ? "" : `${agoText(age)} · `}${tr(`读不出 ${s.feed.unreadable}`, `${s.feed.unreadable} unreadable`)}`,
      to: "/live?tab=market",
    });
    checks.push({
      name: tr("进程自检", "Self-check"),
      icon: <BookCheck />,
      hue: "purple",
      tone: s.reconcile.agreed === false ? "bad" : s.reconcile.agreed ? "good" : "warn",
      verdict: s.reconcile.agreed === null ? tr("尚未核对", "Not yet checked") : s.reconcile.agreed ? tr("一致", "Agrees") : tr("不一致", "Disagrees"),
      detail: tr("内存持仓 vs 交易所查询", "In-memory positions vs venue query"),
      to: "/live?tab=risk",
    });
    checks.push({
      name: tr("交易日志", "Journal"),
      icon: <FileCheck2 />,
      hue: "teal",
      tone: s.journal_lost ? "bad" : "good",
      verdict: s.journal_lost ? tr("无法写入", "Cannot write") : tr("可写", "Writable"),
      detail: s.journal_lost ?? tr("先写日志再发单", "Written before each order is sent"),
      to: "/journal",
    });
  }
  const r = live.data?.reconciliation;
  checks.push({
    name: tr("交易所对账", "Venue reconciliation"),
    icon: <GitCompareArrows />,
    hue: "orange",
    tone: live.isError ? "warn" : !r ? "neutral" : r.verdict === "agree" ? "good" : r.verdict === "disagree" ? "bad" : "warn",
    verdict: live.isError
      ? tr("无法对账", "Cannot reconcile")
      : !r
        ? tr("读取中", "Loading")
        : r.verdict === "agree"
          ? tr("一致", "Agrees")
          : r.verdict === "disagree"
            ? tr(`不一致 ${r.differences.length} 处`, `${r.differences.length} difference${r.differences.length === 1 ? "" : "s"}`)
            : tr("无法判断", "Undetermined"),
    detail: live.data ? tr(`读数 ${agoText(live.data.record_age_ms / 1000)}`, `Read ${agoText(live.data.record_age_ms / 1000)}`) : tr("journal vs 交易所", "journal vs venue"),
    to: "/reconcile",
  });
  if (units.data) {
    const down = units.data.filter((u) => u.ActiveState !== "active");
    checks.push({
      name: tr("服务", "Services"),
      icon: <Server />,
      hue: "green",
      tone: down.length ? "bad" : "good",
      verdict: down.length
        ? tr(`${down.length} 个未运行`, `${down.length} not running`)
        : tr(`${units.data.length}/${units.data.length} 运行`, `${units.data.length}/${units.data.length} running`),
      detail: down.length ? down.map((u) => u.unit.replace(".service", "")).join(tr("、", ", ")) : tr("全部在运行", "All running"),
      to: "/host",
    });
  }
  if (host.data) {
    const h = host.data;
    const pct = (d: (typeof h.disks)[number]) => (d.size && d.used != null ? (d.used / d.size) * 100 : 0);
    const diskPct = h.disks.length ? Math.max(...h.disks.map(pct)) : null;
    const bad = (diskPct ?? 0) >= 90 || h.clock_synced === false;
    checks.push({
      name: tr("主机", "Host"),
      icon: <Cpu />,
      hue: "pink",
      tone: bad ? "bad" : "good",
      verdict: h.clock_synced === false ? tr("时钟未同步", "Clock not synced") : bad ? tr("磁盘将满", "Disk nearly full") : tr("正常", "OK"),
      detail: h.clock_synced
        ? tr(`负载 ${h.load[0]?.toFixed(2)} · 时钟已同步`, `Load ${h.load[0]?.toFixed(2)} · clock synced`)
        : tr(`负载 ${h.load[0]?.toFixed(2)} · 时钟未同步`, `Load ${h.load[0]?.toFixed(2)} · clock not synced`),
      to: "/host",
    });
  }

  const good = checks.filter((c) => c.tone === "good").length;
  const bad = checks.filter((c) => c.tone === "bad").length;
  return (
    <Card
      title={tr("健康检查", "Health checks")}
      icon={<CheckCircle2 />}
      hue="green"
      className="h-full"
      extra={bad ? <Badge tone="bad">{tr(`${bad} 项异常`, `${bad} failing`)}</Badge> : checks.length ? <Badge tone="good">{tr("全部正常", "All OK")}</Badge> : null}
    >
      <div className="mb-4 flex items-center gap-4">
        <Ring value={checks.length ? good : null} max={checks.length || 1} tone={bad ? "bad" : good === checks.length ? "good" : "warn"} label={`${good}/${checks.length}`} />
        <div className="text-sm text-ink-muted">
          {bad
            ? tr(`${bad} 项需要处理，点对应的磁贴进去。`, `${bad} item${bad === 1 ? " needs" : "s need"} attention; click the tile to open it.`)
            : good === checks.length
              ? tr("行情、账目、日志、服务和主机都正常。", "Market data, books, journal, services and host are all OK.")
              : tr("有项目还在读取或无法判断。", "Some items are still loading or cannot be determined.")}
        </div>
      </div>
      <div className="grid grid-cols-2 gap-3">
        {checks.map((c) => (
          <Link
            key={c.name}
            to={c.to}
            className={cx(
              "group flex items-start gap-3 rounded-2xl border p-3 transition-colors hover:border-accent/40 hover:bg-surface-hover",
              c.tone === "bad" ? "border-bad/40 bg-bad/5" : c.tone === "warn" ? "border-warn/40 bg-warn/5" : "border-line",
            )}
          >
            <IconTile icon={c.icon} hue={c.tone === "bad" ? "red" : c.hue} />
            <div className="min-w-0">
              <div className="text-xs text-ink-muted">{c.name}</div>
              <div className={cx("flex items-center gap-1.5 text-sm font-medium", c.tone === "bad" ? "text-bad" : c.tone === "warn" ? "text-warn" : "text-ink")}>
                <StatusDot tone={c.tone} />
                {c.verdict}
              </div>
              <div className="mt-0.5 truncate text-[11px] text-ink-faint">{c.detail}</div>
            </div>
          </Link>
        ))}
      </div>
    </Card>
  );
}

/** What the run has made, sample by sample from the black box. */
export function PnlChart({ day, from, to }: { day?: BlackboxWindow; from: number; to: number }) {
  const net = traderPoints(day, (t) => (t.pnl ? Number(t.pnl.net) : null));
  const start = net[0]?.[0];
  return (
    <Card
      title={tr("盈亏走势", "P&L over time")}
      icon={<TrendingUp />}
      hue="green"
      className="h-full"
      extra={
        <Link to="/blackbox" className="hover:text-ink">
          {tr("黑匣子复盘 →", "Black box review →")}
        </Link>
      }
    >
      {net.length < 2 ? (
        <p className="py-16 text-center text-sm text-ink-faint">{tr("黑匣子里还没有这次运行的盈亏记录。", "The black box has no P&L for this run yet.")}</p>
      ) : (
        <Suspense fallback={chartFallback(260)}>
          <TimeSeries height={260} from={start && start > from ? start : from} to={to} decimals={4} series={[{ name: tr("本次运行盈亏", "Run P&L"), points: net, area: true }]} levels={[{ value: 0 }]} />
        </Suspense>
      )}
      <p className="mt-2 text-xs text-ink-faint">{tr("每 30 秒一个点，来自黑匣子；交易进程重启后从 0 重新计算。", "One point every 30 seconds from the black box; it restarts from 0 when the trader restarts.")}</p>
    </Card>
  );
}

function Alerts() {
  const q = useQuery({ queryKey: ["ops", "alerts", "view"], queryFn: api.alertsView, refetchInterval: 10_000 });
  const active = q.data?.active ?? [];
  return (
    <Card
      title={tr("告警", "Alerts")}
      icon={<Bell />}
      hue="red"
      extra={
        <Link to="/alerts" className="hover:text-ink">
          {tr("全部 →", "All →")}
        </Link>
      }
      className="h-full"
    >
      {q.isError ? (
        <ErrorState error={q.error} what={tr("告警", "alerts")} />
      ) : active.length === 0 ? (
        <div className="flex flex-col items-center gap-2 rounded-2xl bg-good/6 px-4 py-6 text-center">
          <IconTile icon={<CheckCircle2 />} hue="green" size="lg" />
          <div className="text-sm font-medium text-ink">{tr("没有正在发生的告警", "No active alerts")}</div>
          <div className="text-xs text-ink-faint">{tr("每 30 秒检查一次，触发和恢复都会推送到告警频道", "Checked every 30 seconds; raises and clears are pushed to the alert channel")}</div>
        </div>
      ) : (
        <ul className="space-y-2">
          {active.map((a) => (
            <li key={a.key} className="flex items-start gap-3 rounded-2xl border border-bad/30 bg-bad/6 px-3 py-2.5 text-sm">
              <IconTile icon={<AlertTriangle />} hue="red" size="sm" />
              <div className="min-w-0">
                <div className="text-ink">{said(a)}</div>
                <div className="text-xs text-ink-muted">
                  {tr("", "Started ")}
                  <Ago ms={a.since_ms} />
                  {tr(" 开始", "")}
                  {a.silenced_until_ms ? tr(" · 已静默", " · silenced") : ""}
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
      {(q.data?.history.length ?? 0) > 0 && (
        <>
          <div className="mb-2 mt-5 text-xs font-medium text-ink-faint">{tr("最近", "Recent")}</div>
          <ul className="space-y-2 text-xs">
            {q.data!.history.slice(0, 5).map((h, k) => (
              <li key={k} className="flex items-center gap-2">
                <StatusDot tone={h.raised ? "bad" : "good"} />
                <span className="min-w-0 flex-1 truncate text-ink-muted">{said(h)}</span>
                <span className="shrink-0 text-ink-faint">
                  <Ago ms={h.at_ms} />
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </Card>
  );
}

const ACTIVITY: Record<string, { icon: ReactNode; hue: Hue }> = {
  fill: { icon: <ArrowLeftRight />, hue: "blue" },
  refused: { icon: <ShieldAlert />, hue: "orange" },
  operator: { icon: <UserCog />, hue: "purple" },
  funding: { icon: <Layers />, hue: "teal" },
  audit: { icon: <Clock />, hue: "pink" },
};

/** Fills, funding, operator actions and changes to the host, newest first. */
function Activity() {
  const journal = useNewestJournal();
  const records = useQuery({
    queryKey: ["records", journal.id, "activity"],
    queryFn: () => api.records(journal.id as string, ["fill", "operator", "refused", "funding"], 10),
    enabled: Boolean(journal.id),
    refetchInterval: 20_000,
  });
  const audit = useQuery({ queryKey: ["ops", "audit", 10], queryFn: () => api.audit(10), refetchInterval: 60_000 });
  const items = useMemo(() => {
    const out: { at: number; kind: string; label: string; text: string; failed?: boolean }[] = [];
    for (const r of records.data?.records ?? []) {
      if (r.at) out.push({ at: r.at / 1e6, kind: r.kind, label: kindNames()[r.kind] ?? r.kind, text: recordText(r, records.data!.price_scale, records.data!.qty_scale) });
    }
    // Each action is recorded when asked and again with its result; the
    // result is the one worth a line.
    for (const e of audit.data?.entries ?? []) {
      // The verdict is the agent's own word, in English; what is shown
      // is the reader's language.
      const verdict = e.result_en || e.result;
      if (verdict === "requested") continue;
      const failed = /^(refused|failed|error)/.test(verdict);
      const why = pair(e.reason, e.reason_en);
      out.push({ at: e.at_ms, kind: "audit", label: tr("操作", "Action"), text: `${e.op}${why ? tr(`：${why}`, `: ${why}`) : ""}${failed ? tr(" · 未执行", " · not done") : ""}`, failed });
    }
    return out.sort((a, b) => b.at - a.at).slice(0, 9);
  }, [records.data, audit.data]);

  return (
    <Card title={tr("最近动态", "Recent activity")} icon={<Clock />} hue="purple" className="h-full" bodyClassName="px-5 py-3">
      {items.length === 0 ? (
        <p className="py-8 text-center text-sm text-ink-faint">{tr("这次运行还没有成交或操作。", "No fills or actions in this run yet.")}</p>
      ) : (
        <ol className="relative">
          {items.map((i, k) => {
            const a = ACTIVITY[i.kind] ?? ACTIVITY.audit;
            return (
              <li key={k} className="relative flex gap-3 py-2">
                {k < items.length - 1 && <span className="absolute left-3 top-9 h-[calc(100%-1.5rem)] w-px bg-line" />}
                <IconTile icon={a.icon} hue={i.failed ? "orange" : a.hue} size="sm" />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 text-xs">
                    <span className="font-medium text-ink">{i.label}</span>
                    <span className="ml-auto shrink-0 text-ink-faint">
                      <Ago ms={i.at} />
                    </span>
                  </div>
                  <div className="truncate font-mono text-xs text-ink-muted">{i.text}</div>
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </Card>
  );
}

/** The host now, as rings, and the services over the day. */
function Resources({ day, failed, error, from, to }: { day?: BlackboxWindow; failed: boolean; error: unknown; from: number; to: number }) {
  const host = useQuery({ queryKey: ["ops", "host"], queryFn: api.host, refetchInterval: 60_000 });
  const h = host.data;
  const memPct = h?.mem_total && h.mem_available != null ? (1 - h.mem_available / h.mem_total) * 100 : null;
  const disk = h?.disks.length ? Math.max(...h.disks.map((d) => (d.size && d.used != null ? (d.used / d.size) * 100 : 0))) : null;
  const first = day?.host[0]?.at;
  const start = first && first > from ? first : from;
  const ring = (label: string, icon: ReactNode, hue: Hue, value: number | null) => (
    <div className="flex items-center gap-3 rounded-2xl border border-line p-3">
      <Ring value={value} tone={value !== null && value >= 90 ? "bad" : value !== null && value >= 75 ? "warn" : hue} size={56} />
      <div>
        <div className="flex items-center gap-1.5 text-xs text-ink-muted">
          {icon}
          {label}
        </div>
        <div className="text-sm font-medium text-ink">{value === null ? "—" : tr(`已用 ${value.toFixed(0)}%`, `${value.toFixed(0)}% used`)}</div>
      </div>
    </div>
  );
  return (
    <Card
      title={tr("主机资源", "Host resources")}
      icon={<MemoryStick />}
      hue="teal"
      extra={
        <Link to="/host" className="hover:text-ink">
          {tr("主机与服务 →", "Host & services →")}
        </Link>
      }
    >
      <div className="mb-5 grid gap-3 sm:grid-cols-3">
        {ring(tr("内存", "Memory"), <MemoryStick className="h-3.5 w-3.5" />, "teal", memPct)}
        {ring(tr("磁盘", "Disk"), <HardDrive className="h-3.5 w-3.5" />, "blue", disk)}
        <div className="flex items-center gap-3 rounded-2xl border border-line p-3">
          <IconTile icon={<Cpu />} hue="pink" size="lg" />
          <div>
            <div className="text-xs text-ink-muted">{tr("负载 1 / 5 / 15 分钟", "Load 1 / 5 / 15 min")}</div>
            <div className="font-mono text-sm font-medium text-ink">{h ? h.load.map((l) => l.toFixed(2)).join(" / ") : "—"}</div>
          </div>
        </div>
      </div>
      {failed ? (
        <ErrorState error={error} what={tr("黑匣子记录", "black box records")} />
      ) : !day ? (
        chartFallback(190)
      ) : (
        <Suspense fallback={chartFallback(190)}>
          <div className="mb-1 text-xs text-ink-muted">{tr("各服务内存（MiB，最近 24 小时）", "Memory per service (MiB, last 24 h)")}</div>
          <TimeSeries
            height={190}
            from={start}
            to={to}
            decimals={1}
            series={Object.entries(day.units).map(([u, v]) => ({
              name: u.replace(".service", ""),
              points: v.curve.map((p) => [p.at, p.mem == null ? null : p.mem / 2 ** 20] as [number, number | null]),
            }))}
          />
        </Suspense>
      )}
    </Card>
  );
}

function NoHost() {
  return (
    <div className="space-y-5">
      <PageHeader
        title={tr("总览", "Overview")}
        description={tr("这个 deck 没有连接主机代理，只能看研究数据。", "This deck is not connected to a host agent; only research data is available.")}
      />
      <Card>
        <p className="text-sm text-ink-muted">
          {tr("设置 ", "Set ")}
          <code className="font-mono text-ink">OQ_DECK_AGENT_SOCKET</code>
          {tr(
            " 指向交易主机上的 oq-agent 后，这里会显示交易进程、告警和健康检查。回测记录与参数扫描在左侧「研究」里。",
            " to the oq-agent on the trading host and the trader, alerts and health checks show here. Backtests and sweeps are under \"Research\" on the left.",
          )}
        </p>
      </Card>
    </div>
  );
}
