import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, ListOrdered, ShieldCheck, Wallet } from "lucide-react";

import { api, type RecordsPage, type TraderStatus } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { KIND_NAMES, TraderActions, lotsText, recordText, useCaps, useNewestJournal, useTrader } from "@/features/trading";
import { TimeSeries } from "@/ui/charts";
import { Badge, Button, Card, Freshness, KV, PageHeader, Segmented, Stat, TabBar, Table, cx, fmtDuration, fmtTime, useTab } from "@/ui/kit";

import { Kpis, PnlChart, PriceAndOrders, useDay } from "./Overview";

const TABS = ["positions", "fills", "market", "risk"] as const;

/**
 * Everything about the live trader on one page (docs/UI-V4 §4.3): what
 * it holds and has resting, what filled, the market it sees, and the
 * limits it trades under — with halt and resume beside its status.
 */
export function Trading() {
  const caps = useCaps();
  const writable = caps.data?.writes.available === true;
  const status = useTrader();
  const tab = useTab(TABS);
  const s = status.data;
  const day = useDay();

  return (
    <div>
      <PageHeader
        title="实盘"
        description={s ? `${s.strategy} · ${s.symbol} · ${s.deployment === "Live" ? "主网" : "测试网"} · 订单前缀 ${s.prefix}` : "交易进程的持仓、挂单、成交、行情与风控。"}
        meta={<Freshness at={status.dataUpdatedAt} fetching={status.isFetching} staleAfterS={30} onRefresh={() => status.refetch()} />}
        actions={<TraderActions s={s} writable={writable} />}
      />
      {status.isError ? (
        <Card tone="bad">
          <ErrorState error={status.error} what="交易进程状态" />
        </Card>
      ) : !s ? (
        <Skeleton tiles={4} rows={4} />
      ) : (
        <div className="space-y-5">
          {s.halted && (
            <div className="flex items-center gap-3 rounded-[var(--radius-card)] border border-bad/40 bg-bad/8 px-4 py-3 text-sm">
              <AlertTriangle className="h-4 w-4 text-bad" />
              <span className="text-ink">已停机：{s.halt_reason ?? "未说明原因"}</span>
              {!s.resume_allowed && <span className="text-ink-muted">（这个进程不允许从控制台解除停机）</span>}
            </div>
          )}
          <Kpis s={s} day={day.data} />
          <div>
            <TabBar
              tabs={[
                { key: "positions", label: "持仓与挂单" },
                { key: "fills", label: "成交与订单" },
                { key: "market", label: "行情" },
                { key: "risk", label: "风控与计数" },
              ]}
            />
            {tab === "positions" && (
              <div className="space-y-5">
                <div className="grid gap-5 xl:grid-cols-2">
                  <PriceAndOrders s={s} />
                  <PnlChart day={day.data} from={day.from} to={day.to} />
                </div>
                <Positions s={s} />
              </div>
            )}
            {tab === "fills" && <Fills />}
            {tab === "market" && <Market s={s} />}
            {tab === "risk" && <Risk s={s} />}
          </div>
        </div>
      )}
    </div>
  );
}

function scaled(value: number | null, scale: number | undefined) {
  if (value === null) return "—";
  if (scale === undefined) return String(value);
  return (value / 10 ** scale).toFixed(scale);
}

function Positions({ s }: { s: TraderStatus }) {
  const orders = useQuery({ queryKey: ["ops", "orders"], queryFn: api.orders, refetchInterval: 10_000 });
  const journal = useNewestJournal();
  const belief = (journal.data ?? []).find((j) => j.id === journal.id)?.belief;
  const entryOf = (side: string) => {
    const leg = belief?.legs.find(([l]) => l.toUpperCase() === side.toUpperCase());
    return leg ? scaled(leg[2], belief?.price_scale) : "—";
  };
  const list = orders.data?.orders ?? [];
  const opening = list.filter((o) => !o.closing).length;

  return (
    <div className="grid gap-5 xl:grid-cols-5">
      <Card title="持仓" icon={<Wallet className="h-4 w-4" />} className="xl:col-span-2" bodyClassName="p-0" extra={belief?.hedged ? <Badge>双向持仓</Badge> : undefined}>
        {s.positions.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-ink-faint">空仓</p>
        ) : (
          <Table head={["腿", "数量", "开仓均价"]}>
            {s.positions.map((p) => (
              <tr key={p.side}>
                <td>
                  <Badge>{p.side === "LONG" ? "多" : p.side === "SHORT" ? "空" : p.side}</Badge>
                </td>
                <td className="font-mono">{p.amount.replace("-", "")}</td>
                <td className="font-mono">{entryOf(p.side)}</td>
              </tr>
            ))}
          </Table>
        )}
        <p className="border-t border-line px-4 py-2.5 text-xs text-ink-faint">数量来自交易进程，均价来自它的 journal 重建（{journal.id ?? "—"}）。</p>
      </Card>
      <Card
        title="挂单"
        icon={<ListOrdered className="h-4 w-4" />}
        className="xl:col-span-3"
        bodyClassName="p-0"
        extra={
          <span>
            开仓 {opening} · 平仓 {list.length - opening}
            {s.limits ? ` · 上限 ${s.limits.max_working}` : ""}
          </span>
        }
      >
        {orders.isError ? (
          <div className="p-4">
            <ErrorState error={orders.error} what="挂单" />
          </div>
        ) : list.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-ink-faint">交易进程认为当前没有挂单</p>
        ) : (
          <div className="max-h-[26rem] overflow-auto">
            <Table head={["方向", "价格", "数量", "类型", "客户端订单号"]}>
              {[...list]
                .sort((a, b) => (b.price_ticks ?? 0) - (a.price_ticks ?? 0))
                .map((o) => (
                  <tr key={o.client_id}>
                    {/* Neutral: a sell is neither good nor bad (UI-BRIEF §8). */}
                    <td className="text-ink">{o.side === "BUY" ? "买" : o.side === "SELL" ? "卖" : "—"}</td>
                    <td className="font-mono">{scaled(o.price_ticks, s.price_scale)}</td>
                    <td className="font-mono">{scaled(o.qty_lots, s.qty_scale)}</td>
                    <td>{o.closing ? <Badge tone="accent">平仓 · 停机保留</Badge> : <span className="text-ink-muted">开仓</span>}</td>
                    <td className="font-mono text-xs text-ink-faint">{o.client_id}</td>
                  </tr>
                ))}
            </Table>
          </div>
        )}
        <p className="border-t border-line px-4 py-2.5 text-xs text-ink-faint">这是交易进程自己记的账；与交易所实际挂单的比对见「对账与归因」。</p>
      </Card>
    </div>
  );
}

const ORDER_KINDS = ["submitted", "outcome", "cancelled", "fill", "refused"];

function Fills() {
  const journal = useNewestJournal();
  const [only, setOnly] = useState<"fill" | "orders">("fill");
  const [cursor, setCursor] = useState<number | null>(null);
  const kinds = only === "fill" ? ["fill"] : ORDER_KINDS;
  const q = useQuery({
    queryKey: ["records", journal.id, kinds.join(","), cursor],
    queryFn: () => api.records(journal.id as string, kinds, 100, cursor),
    enabled: Boolean(journal.id),
    refetchInterval: cursor === null ? 30_000 : false,
  });
  return (
    <Card
      title={only === "fill" ? "成交" : "订单事件"}
      bodyClassName="p-0"
      extra={
        <>
          <span className="font-mono">{journal.id}</span>
          <Segmented
            value={only}
            onChange={(v) => {
              setOnly(v);
              setCursor(null);
            }}
            options={[
              { value: "fill", label: "只看成交" },
              { value: "orders", label: "全部订单事件" },
            ]}
          />
        </>
      }
    >
      {!journal.id ? (
        <div className="p-4">
          <Empty title="还没有可读的 journal。" next="交易进程启动后会在 journal 目录写一份。" />
        </div>
      ) : q.isLoading ? (
        <div className="p-4">
          <Skeleton rows={5} />
        </div>
      ) : q.isError ? (
        <div className="p-4">
          <ErrorState error={q.error} what="成交记录" />
        </div>
      ) : (
        <>
          <RecordRows page={q.data!} empty={only === "fill" ? "这次运行还没有成交。" : "这次运行还没有订单事件。"} />
          <div className="flex items-center justify-between border-t border-line px-4 py-2.5 text-xs text-ink-faint">
            <span>共 {q.data!.total} 条</span>
            <div className="flex gap-2">
              {cursor !== null && (
                <Button size="sm" variant="ghost" onClick={() => setCursor(null)}>
                  回到最新
                </Button>
              )}
              {q.data!.next_before !== null && (
                <Button size="sm" onClick={() => setCursor(q.data!.next_before)}>
                  更早
                </Button>
              )}
            </div>
          </div>
        </>
      )}
    </Card>
  );
}

/** Journal records as rows. */
export function RecordRows({ page, empty }: { page: RecordsPage; empty: string }) {
  if (!page.records.length) return <p className="px-4 py-6 text-center text-sm text-ink-faint">{empty}</p>;
  return (
    <div className="max-h-[32rem] overflow-auto">
      <Table head={["时间", "类型", "内容", "订单号"]} dense>
        {page.records.map((r) => (
          <tr key={r.seq}>
            <td className="whitespace-nowrap font-mono text-xs text-ink-muted">{r.at ? fmtTime(r.at / 1e6) : "—"}</td>
            <td className="whitespace-nowrap">
              <Badge tone={r.kind === "fill" ? "accent" : r.kind === "refused" ? "warn" : "neutral"}>{KIND_NAMES[r.kind] ?? r.kind}</Badge>
            </td>
            <td className="font-mono text-xs text-ink">{recordText(r, page.price_scale, page.qty_scale)}</td>
            <td className="font-mono text-xs text-ink-faint">{String(r.fields.client_id ?? "")}</td>
          </tr>
        ))}
      </Table>
    </div>
  );
}

const STALE_S = 10;

function Market({ s }: { s: TraderStatus }) {
  const journal = useNewestJournal();
  const q = useQuery({
    queryKey: ["records", journal.id, "tick", 600],
    queryFn: () => api.records(journal.id as string, ["tick"], 600),
    enabled: Boolean(journal.id),
    refetchInterval: 10_000,
  });
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  const stats = useMemo(() => {
    const rs = [...(q.data?.records ?? [])].reverse();
    if (rs.length < 2) return null;
    const seen = rs.map((r) => Number(r.fields.seen));
    const at = rs.map((r) => r.at ?? 0);
    const gaps = seen.slice(1).map((v, i) => (v - seen[i]) / 1e9);
    const lag = seen.map((v, i) => (v - at[i]) / 1e9).sort((a, b) => a - b);
    const pick = (p: number) => lag[Math.min(lag.length - 1, Math.floor(p * lag.length))];
    return {
      n: rs.length,
      span: (seen[seen.length - 1] - seen[0]) / 1e9,
      maxGap: Math.max(...gaps),
      gaps5: gaps.filter((g) => g > 5).length,
      backwards: at.slice(1).filter((a, i) => a < at[i]).length,
      p50: pick(0.5),
      p95: pick(0.95),
      series: rs.map((r) => [Number(r.fields.seen) / 1e6, Number(r.fields.last) / 10 ** (q.data?.price_scale ?? 0)] as [number, number]),
    };
  }, [q.data]);

  if (q.isLoading) return <Skeleton tiles={4} rows={3} />;
  if (q.isError) return <ErrorState error={q.error} what="行情" />;
  const ticks = q.data?.records ?? [];
  if (!ticks.length) return <Empty title="这次运行的 journal 里还没有行情。" next="交易进程每个时间窗写一条 tick；刚启动时要等几秒。" />;
  const ps = q.data!.price_scale;
  const px = (v: unknown) => (Number(v) / 10 ** ps).toFixed(ps);
  const latest = ticks[0];
  const ageS = (now * 1e6 - Number(latest.fields.seen)) / 1e9;
  const stale = ageS > STALE_S;

  return (
    <div className="space-y-5">
      {stale && (
        <div className="flex items-center gap-2 rounded-[var(--radius-card)] border border-bad/40 bg-bad/8 px-4 py-3 text-sm text-ink">
          <AlertTriangle className="h-4 w-4 text-bad" />
          这个价格已经 {fmtDuration(ageS)}没更新——不是当前价。行情可能断了，看下方的计数和告警。
        </div>
      )}
      <div className={cx("grid gap-4 sm:grid-cols-2 xl:grid-cols-4", stale && "opacity-60")}>
        <Stat label="最新成交价" value={px(latest.fields.last)} sub={`${fmtDuration(ageS)}前`} tone={stale ? "bad" : undefined} />
        <Stat label="买一 / 卖一" value={`${px(latest.fields.bid)} / ${px(latest.fields.ask)}`} />
        <Stat label="到达延迟 p50 / p95" value={stats ? `${stats.p50.toFixed(2)} / ${stats.p95.toFixed(2)} 秒` : "—"} sub="交易所时间 → 进程收到" />
        <Stat label="最长间隔" value={stats ? `${stats.maxGap.toFixed(1)} 秒` : "—"} sub={stats ? `超过 5 秒的间隔 ${stats.gaps5} 次` : undefined} tone={stats && stats.maxGap > 10 ? "warn" : undefined} />
      </div>
      <Card title={`价格（最近 ${stats?.n ?? 0} 条，${stats ? fmtDuration(stats.span) : ""}）`}>
        {stats && <TimeSeries height={220} series={[{ name: "last", points: stats.series, area: true }]} />}
      </Card>
      <Card title="行情计数（本次运行）">
        <KV
          cols={3}
          items={[
            ["深度更新", s.feed.depth.toLocaleString()],
            ["逐笔成交", s.feed.trades.toLocaleString()],
            ["交易所时间倒退", `${s.feed.out_of_order.toLocaleString()} 次`],
            ["静默窗口", s.feed.quiet.toLocaleString()],
            ["快照", s.feed.snapshots.toLocaleString()],
            [<span key="r">重同步</span>, <span key="v" className={s.feed.resyncs > 0 ? "text-warn" : ""}>{s.feed.resyncs}</span>],
            [<span key="u">读不出的消息</span>, <span key="v" className={s.feed.unreadable > 0 ? "text-bad" : ""}>{s.feed.unreadable}</span>],
            ["本段时间戳倒退", stats ? `${stats.backwards} 次` : "—"],
          ]}
        />
      </Card>
    </div>
  );
}

function Risk({ s }: { s: TraderStatus }) {
  const l = s.limits;
  const c = s.counters;
  return (
    <div className="grid gap-5 xl:grid-cols-2">
      <Card title="风控限额（生效中）" icon={<ShieldCheck className="h-4 w-4" />}>
        {!l ? (
          <p className="text-sm text-ink-muted">这个版本的交易进程不报告限额；发布新版本后显示。</p>
        ) : (
          <KV
            items={[
              ["单笔数量上限", lotsText(l.max_order_qty, s.qty_scale)],
              ["持仓上限", lotsText(l.max_position_qty, s.qty_scale)],
              ["单笔名义金额上限", l.max_order_notional],
              ["限价偏离标记价上限", `${(l.price_band_ppb / 1e7).toFixed(2)}%`],
              ["同时挂单上限", `${l.max_working}（当前 ${s.resting}）`],
              ["发单频率上限", `${l.max_rate} 单 / ${(l.rate_window_ns / 1e9).toFixed(0)} 秒`],
            ]}
          />
        )}
      </Card>
      <Card title="本次运行计数">
        <KV
          items={[
            ["发出订单", String(c.sent ?? 0)],
            ["成交", String(c.fills ?? 0)],
            ["连接断开", String(c.disconnects ?? 0)],
            [<span key="f">外来订单</span>, <span key="v" className={(c.foreign_orders ?? 0) > 0 ? "text-bad" : ""}>{c.foreign_orders ?? 0}</span>],
            ["无法入账的回报", String(c.unbookable_reports ?? 0)],
            [
              "进程自检",
              <span key="v" className={s.reconcile.agreed === false ? "text-bad" : ""}>
                {s.reconcile.agreed === null ? "尚未核对" : s.reconcile.agreed ? "一致" : "不一致"}
                {s.reconcile.at_ns ? `（${fmtTime(s.reconcile.at_ns / 1e6, false)}）` : ""} · 累计不一致 {s.reconcile.mismatches} · 读取失败 {s.reconcile.unread}
              </span>,
            ],
          ]}
        />
      </Card>
      {Object.keys(s.waiting_on).length > 0 && (
        <Card title="策略在等什么" className="xl:col-span-2">
          <div className="flex flex-wrap gap-2">
            {Object.entries(s.waiting_on).map(([k, v]) => (
              <Badge key={k}>
                <span className="font-mono">
                  {k} {v}
                </span>
              </Badge>
            ))}
          </div>
        </Card>
      )}
    </div>
  );
}
