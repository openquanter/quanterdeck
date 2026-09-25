import { Suspense, lazy, useMemo, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, ArrowRight, Bell, CheckCircle2, Clock, Cpu, Radio } from "lucide-react";

import { api, type TraderStatus } from "@/api/client";
import { ErrorState } from "@/components/States";
import { KIND_NAMES, TraderActions, lotsText, recordText, useCaps, useNewestJournal, useTrader } from "@/features/trading";
import { Ago, Badge, Card, Freshness, Money, PageHeader, Stat, StatusDot, cx, fmtDuration, type Tone } from "@/ui/kit";

// The chart library is most of the bundle; the overview's numbers should
// not wait for it.
const TimeSeries = lazy(() => import("@/ui/charts").then((m) => ({ default: m.TimeSeries })));

/**
 * The first screen answers one question — is everything all right — and
 * every "no" links to where it is dealt with (docs/UI-V4 §4.2). Research
 * numbers live with the research.
 */
export function Overview() {
  const caps = useCaps();
  const ops = caps.data?.ops?.available === true;
  const writable = caps.data?.writes.available === true;
  const status = useTrader(ops);

  if (caps.data && !ops) return <NoHost />;

  return (
    <div className="space-y-5">
      <PageHeader
        title="总览"
        description="交易主机此刻是否一切正常；任何一项不正常都可以点进去处理。"
        meta={<Freshness at={status.dataUpdatedAt} fetching={status.isFetching} staleAfterS={30} onRefresh={() => status.refetch()} />}
      />
      <Banner status={status.data} failed={status.isError} writable={writable} />
      {status.data && <Kpis s={status.data} />}
      <div className="grid gap-5 xl:grid-cols-5">
        <div className="xl:col-span-3">
          <Health s={status.data} failed={status.isError} />
        </div>
        <div className="xl:col-span-2">
          <Alerts />
        </div>
      </div>
      <div className="grid gap-5 xl:grid-cols-5">
        <div className="xl:col-span-3">
          <Resources />
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
  return (
    <div
      className={cx(
        "flex flex-wrap items-center gap-4 rounded-[var(--radius-card)] border px-5 py-4",
        tone === "bad" ? "border-bad/40 bg-bad/8" : tone === "good" ? "border-good/25 bg-good/5" : "border-line bg-surface",
      )}
    >
      <div className={cx("flex h-10 w-10 items-center justify-center rounded-full", tone === "bad" ? "bg-bad/15 text-bad" : "bg-good/15 text-good")}>
        {tone === "bad" ? <AlertTriangle className="h-5 w-5" /> : <Radio className="h-5 w-5" />}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2 text-lg font-semibold text-ink">
          {failed ? "交易进程无应答" : !s ? "读取中…" : s.halted ? "交易进程已停机" : "交易进程运行中"}
          {s && <Badge tone={s.deployment === "Live" ? "bad" : "accent"}>{s.deployment === "Live" ? "主网" : "测试网"}</Badge>}
        </div>
        <div className="mt-0.5 text-sm text-ink-muted">
          {failed
            ? "控制口没有回应：进程可能没在运行，或卡住了。去「主机与服务」看服务状态和日志。"
            : s?.halted
              ? `原因：${s.halt_reason ?? "未说明"}`
              : s
                ? [
                    `${s.strategy} · ${s.symbol}`,
                    up !== null && `已运行 ${fmtDuration(up)}`,
                    lastTickAge !== null && `最近行情 ${fmtDuration(lastTickAge)}前`,
                  ]
                    .filter(Boolean)
                    .join(" · ")
                : " "}
        </div>
      </div>
      <div className="flex gap-2">
        <TraderActions s={s} writable={writable} compact />
        <Link to={failed ? "/host" : "/live"} className="inline-flex h-8 items-center gap-1.5 rounded-md border border-line-strong bg-surface-raised px-3 text-sm text-ink hover:bg-surface-hover">
          {failed ? "主机与服务" : "查看实盘"} <ArrowRight className="h-4 w-4" />
        </Link>
      </div>
    </div>
  );
}

export function Kpis({ s }: { s: TraderStatus }) {
  const long = s.positions.filter((p) => Number(p.amount) > 0);
  const short = s.positions.filter((p) => Number(p.amount) < 0);
  return (
    <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <Stat
        label="本次运行盈亏"
        value={s.pnl ? <Money value={s.pnl.net} signed /> : "—"}
        sub={s.pnl ? `已实现 ${s.pnl.realized} · 手续费 ${s.pnl.fees}` : "此版本的交易进程不报告盈亏"}
        help="run_pnl"
      />
      <Stat label="权益" value={s.pnl ? <Money value={s.pnl.equity} /> : "—"} sub="按最近标记价" />
      <Stat
        label="持仓"
        value={s.positions.length === 0 ? "空仓" : `${long.length ? `多 ${long.map((p) => p.amount).join(" ")}` : ""}${long.length && short.length ? " / " : ""}${short.length ? `空 ${short.map((p) => p.amount.replace("-", "")).join(" ")}` : ""}`}
        sub={s.symbol}
        help="hedged"
      />
      <Stat
        label="挂单"
        value={String(s.resting)}
        sub={s.limits ? `上限 ${s.limits.max_working} · 持仓上限 ${lotsText(s.limits.max_position_qty, s.qty_scale)}` : undefined}
      />
    </div>
  );
}

type Check = { name: string; tone: Tone; verdict: string; detail?: ReactNode; to: string; help?: string };

function Health({ s, failed }: { s: TraderStatus | undefined; failed: boolean }) {
  const live = useQuery({ queryKey: ["live", "latest"], queryFn: api.liveLatest, refetchInterval: 30_000, retry: false });
  const units = useQuery({ queryKey: ["ops", "units"], queryFn: api.units, refetchInterval: 30_000 });
  const host = useQuery({ queryKey: ["ops", "host"], queryFn: api.host, refetchInterval: 60_000 });

  const checks: Check[] = [];
  if (failed) {
    checks.push({ name: "交易进程", tone: "bad", verdict: "无应答", detail: "控制口没有回应", to: "/host" });
  } else if (s) {
    const age = s.last_tick ? (s.now_ns - s.last_tick.local_ns) / 1e9 : null;
    checks.push({
      name: "行情",
      tone: age === null ? "warn" : age > 120 || s.feed.unreadable > 0 ? "bad" : "good",
      verdict: age === null ? "尚无行情" : age > 120 ? "行情中断" : "正常",
      detail: `${age === null ? "" : `最近一笔 ${fmtDuration(age)}前 · `}读不出 ${s.feed.unreadable} · 重同步 ${s.feed.resyncs}`,
      to: "/live?tab=market",
    });
    checks.push({
      name: "进程自检",
      tone: s.reconcile.agreed === false ? "bad" : s.reconcile.agreed ? "good" : "warn",
      verdict: s.reconcile.agreed === null ? "尚未核对" : s.reconcile.agreed ? "一致" : "不一致",
      detail: "进程内存里的持仓 vs 它定时向交易所查询的结果",
      to: "/live?tab=risk",
      help: "self_check",
    });
    checks.push({
      name: "交易日志",
      tone: s.journal_lost ? "bad" : "good",
      verdict: s.journal_lost ? "无法写入" : "可写",
      detail: s.journal_lost ?? "每个决策先写日志再发单",
      to: "/journal",
    });
  }
  const r = live.data?.reconciliation;
  checks.push({
    name: "交易所对账",
    tone: live.isError ? "warn" : !r ? "neutral" : r.verdict === "agree" ? "good" : r.verdict === "disagree" ? "bad" : "warn",
    verdict: live.isError ? "无法对账" : !r ? "读取中" : r.verdict === "agree" ? "一致" : r.verdict === "disagree" ? `不一致（${r.differences.length} 处）` : "无法判断",
    detail: live.data ? `journal 重建的持仓 vs 交易所最新读数（${fmtDuration(live.data.record_age_ms / 1000)}前）` : "journal 重建的持仓 vs 交易所最新读数",
    to: "/reconcile",
    help: "belief",
  });
  if (units.data) {
    const down = units.data.filter((u) => u.ActiveState !== "active");
    checks.push({
      name: "服务",
      tone: down.length ? "bad" : "good",
      verdict: down.length ? `${down.length} 个未运行` : `${units.data.length} 个全部运行`,
      detail: down.length ? down.map((u) => u.unit.replace(".service", "")).join("、") : units.data.map((u) => u.unit.replace(".service", "")).join(" · "),
      to: "/host",
    });
  }
  if (host.data) {
    const h = host.data;
    const memPct = h.mem_total && h.mem_available != null ? (1 - h.mem_available / h.mem_total) * 100 : null;
    const pct = (d: (typeof h.disks)[number]) => (d.size && d.used != null ? (d.used / d.size) * 100 : 0);
    const fullest = [...h.disks].sort((a, b) => pct(b) - pct(a))[0];
    const diskPct = fullest ? pct(fullest) : null;
    const bad = (diskPct ?? 0) >= 90 || h.clock_synced === false;
    checks.push({
      name: "主机",
      tone: bad ? "bad" : "good",
      verdict: h.clock_synced === false ? "时钟未同步" : bad ? "磁盘将满" : "正常",
      detail: `负载 ${h.load[0]?.toFixed(2)} · 内存 ${memPct?.toFixed(0) ?? "—"}% · 磁盘 ${diskPct?.toFixed(0) ?? "—"}% · 时钟${h.clock_synced ? "已同步" : "未同步"}`,
      to: "/host",
    });
  }

  const bad = checks.filter((c) => c.tone === "bad").length;
  return (
    <Card
      title="健康检查"
      icon={<CheckCircle2 className="h-4 w-4" />}
      extra={bad ? <Badge tone="bad">{bad} 项异常</Badge> : checks.length ? <Badge tone="good">全部正常</Badge> : null}
      bodyClassName="p-0"
    >
      <ul>
        {checks.map((c) => (
          <li key={c.name} className="border-b border-line/60 last:border-0">
            <Link to={c.to} className="group flex items-center gap-3 px-4 py-3 hover:bg-surface-hover/50">
              <StatusDot tone={c.tone} />
              <span className="w-24 shrink-0 text-sm text-ink">{c.name}</span>
              <span className={cx("w-32 shrink-0 text-sm font-medium", c.tone === "bad" ? "text-bad" : c.tone === "warn" ? "text-warn" : "text-ink")}>{c.verdict}</span>
              <span className="min-w-0 flex-1 truncate text-xs text-ink-faint">{c.detail}</span>
              <ArrowRight className="h-4 w-4 shrink-0 text-ink-faint opacity-0 group-hover:opacity-100" />
            </Link>
          </li>
        ))}
      </ul>
    </Card>
  );
}

function Alerts() {
  const q = useQuery({ queryKey: ["ops", "alerts", "view"], queryFn: api.alertsView, refetchInterval: 10_000 });
  const active = q.data?.active ?? [];
  return (
    <Card
      title="告警"
      icon={<Bell className="h-4 w-4" />}
      extra={
        <Link to="/alerts" className="hover:text-ink">
          全部 →
        </Link>
      }
      className="h-full"
    >
      {q.isError ? (
        <ErrorState error={q.error} what="告警" />
      ) : active.length === 0 ? (
        <div className="flex items-center gap-2 rounded-md bg-good/8 px-3 py-2.5 text-sm text-good">
          <CheckCircle2 className="h-4 w-4" /> 没有正在发生的告警
        </div>
      ) : (
        <ul className="space-y-2">
          {active.map((a) => (
            <li key={a.key} className="flex items-start gap-2 rounded-md border border-bad/30 bg-bad/8 px-3 py-2 text-sm">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-bad" />
              <div className="min-w-0">
                <div className="text-ink">{a.message}</div>
                <div className="text-xs text-ink-muted">
                  <Ago ms={a.since_ms} /> 开始{a.silenced_until_ms ? " · 已静默" : ""}
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
      {(q.data?.history.length ?? 0) > 0 && (
        <>
          <div className="mb-1.5 mt-4 text-xs text-ink-faint">最近</div>
          <ul className="space-y-1.5 text-xs">
            {q.data!.history.slice(0, 5).map((h, k) => (
              <li key={k} className="flex gap-2">
                <span className={h.raised ? "text-bad" : "text-good"}>{h.raised ? "触发" : "恢复"}</span>
                <span className="min-w-0 flex-1 truncate text-ink-muted">{h.message}</span>
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

/** Fills, operator actions and service changes, newest first. */
function Activity() {
  const journal = useNewestJournal();
  const fills = useQuery({
    queryKey: ["records", journal.id, "activity"],
    queryFn: () => api.records(journal.id as string, ["fill", "operator", "refused"], 8),
    enabled: Boolean(journal.id),
    refetchInterval: 30_000,
  });
  const audit = useQuery({ queryKey: ["ops", "audit", 10], queryFn: () => api.audit(10), refetchInterval: 60_000 });
  const items = useMemo(() => {
    const out: { at: number; kind: string; text: string; tone: Tone }[] = [];
    for (const r of fills.data?.records ?? []) {
      if (r.at) out.push({ at: r.at / 1e6, kind: KIND_NAMES[r.kind] ?? r.kind, text: recordText(r, fills.data!.price_scale, fills.data!.qty_scale), tone: r.kind === "refused" ? "warn" : "accent" });
    }
    // Each action is recorded when asked and again with its result; the
    // result is the one worth a line.
    for (const e of audit.data?.entries ?? []) {
      if (e.result === "requested") continue;
      const failed = /^(refused|failed|error)/.test(e.result);
      const result = e.result === "done" ? "完成" : failed ? `未执行（${e.result.replace(/^refused: /, "")}）` : e.result;
      out.push({ at: e.at_ms, kind: "操作", text: `${e.op}${e.reason ? `：${e.reason}` : ""} · ${result}`, tone: failed ? "warn" : "neutral" });
    }
    return out.sort((a, b) => b.at - a.at).slice(0, 10);
  }, [fills.data, audit.data]);

  return (
    <Card title="最近动态" icon={<Clock className="h-4 w-4" />} className="h-full" bodyClassName="p-0">
      {items.length === 0 ? (
        <p className="px-4 py-6 text-center text-sm text-ink-faint">这次运行还没有成交或操作。</p>
      ) : (
        <ul>
          {items.map((i, k) => (
            <li key={k} className="flex items-start gap-3 border-b border-line/60 px-4 py-2.5 text-xs last:border-0">
              <span className="mt-1">
                <StatusDot tone={i.tone} />
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex gap-2">
                  <span className="shrink-0 text-ink-muted">{i.kind}</span>
                  <span className="truncate font-mono text-ink">{i.text}</span>
                </div>
              </div>
              <span className="shrink-0 text-ink-faint">
                <Ago ms={i.at} />
              </span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

/** The last day from the black box: memory of each service and host load. */
function Resources() {
  const to = Math.floor(Date.now() / 60_000) * 60_000;
  const from = to - 24 * 3_600_000;
  const q = useQuery({ queryKey: ["blackbox", "overview", to], queryFn: () => api.blackbox(from, to, 240), refetchInterval: 5 * 60_000 });
  const w = q.data;
  // The axis starts where the recording does when that is inside the day,
  // rather than drawing hours of nothing.
  const first = w?.host[0]?.at;
  const start = first && first > from ? first : from;
  return (
    <Card
      title="资源（最近 24 小时）"
      icon={<Cpu className="h-4 w-4" />}
      extra={
        <Link to="/blackbox" className="hover:text-ink">
          黑匣子复盘 →
        </Link>
      }
    >
      {q.isError ? (
        <ErrorState error={q.error} what="黑匣子记录" />
      ) : !w ? (
        <div className="h-[180px] animate-pulse rounded bg-surface-raised" />
      ) : (
        <Suspense fallback={<div className="h-[190px] animate-pulse rounded bg-surface-raised" />}>
        <div className="grid gap-4 md:grid-cols-2">
          <div>
            <div className="mb-1 text-xs text-ink-muted">各服务内存（MiB）</div>
            <TimeSeries
              height={170}
              from={start}
              to={to}
              decimals={1}
              series={Object.entries(w.units).map(([u, v]) => ({
                name: u.replace(".service", ""),
                points: v.curve.map((p) => [p.at, p.mem == null ? null : p.mem / 2 ** 20] as [number, number | null]),
              }))}
            />
          </div>
          <div>
            <div className="mb-1 text-xs text-ink-muted">主机负载（1 分钟）</div>
            <TimeSeries height={170} from={start} to={to} series={[{ name: "负载", area: true, points: w.host.map((h) => [h.at, h.host.load?.[0] ?? null]) }]} />
          </div>
        </div>
        </Suspense>
      )}
    </Card>
  );
}

function NoHost() {
  return (
    <div className="space-y-5">
      <PageHeader title="总览" description="这个 deck 没有连接主机代理，只能看研究数据。" />
      <Card>
        <p className="text-sm text-ink-muted">
          设置 <code className="font-mono text-ink">OQ_DECK_AGENT_SOCKET</code> 指向交易主机上的 oq-agent 后，这里会显示交易进程、告警和健康检查。回测记录与参数扫描在左侧「研究」里。
        </p>
      </Card>
    </div>
  );
}
