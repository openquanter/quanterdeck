import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, ListOrdered, ShieldCheck, Wallet } from "lucide-react";

import { api, type RecordsPage, type TraderStatus } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { kindNames, TraderActions, lotsText, recordText, useCaps, useNewestJournal, useTrader } from "@/features/trading";
import { TimeSeries } from "@/ui/charts";
import { tr } from "@/i18n";
import { Badge, Button, Card, Freshness, KV, PageHeader, Segmented, Stat, TabBar, Table, agoText, cx, fmtDuration, fmtTime, useTab } from "@/ui/kit";

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
  const net = s?.deployment === "Live" ? tr("主网", "Mainnet") : tr("测试网", "Testnet");

  return (
    <div>
      <PageHeader
        title={tr("实盘", "Live")}
        description={
          s
            ? tr(
                `${s.strategy} · ${s.symbol} · ${net} · 订单前缀 ${s.prefix}`,
                `${s.strategy} · ${s.symbol} · ${net} · order prefix ${s.prefix}`,
              )
            : tr("交易进程的持仓、挂单、成交、行情与风控。", "The trader's positions, open orders, fills, market data and risk limits.")
        }
        meta={<Freshness at={status.dataUpdatedAt} fetching={status.isFetching} staleAfterS={30} onRefresh={() => status.refetch()} />}
        actions={<TraderActions s={s} writable={writable} />}
      />
      {status.isError ? (
        <Card tone="bad">
          <ErrorState error={status.error} what={tr("交易进程状态", "trader status")} />
        </Card>
      ) : !s ? (
        <Skeleton tiles={4} rows={4} />
      ) : (
        <div className="space-y-5">
          {s.halted && (
            <div className="flex items-center gap-3 rounded-[var(--radius-card)] border border-bad/40 bg-bad/8 px-4 py-3 text-sm">
              <AlertTriangle className="h-4 w-4 text-bad" />
              <span className="text-ink">
                {tr("已停机：", "Halted: ")}
                {s.halt_reason ?? tr("未说明原因", "no reason given")}
              </span>
              {!s.resume_allowed && (
                <span className="text-ink-muted">{tr("（这个进程不允许从控制台解除停机）", "(this process does not allow resuming from the deck)")}</span>
              )}
            </div>
          )}
          <Kpis s={s} day={day.data} />
          <div>
            <TabBar
              tabs={[
                { key: "positions", label: tr("持仓与挂单", "Positions & orders") },
                { key: "fills", label: tr("成交与订单", "Fills & order events") },
                { key: "market", label: tr("行情", "Market") },
                { key: "risk", label: tr("风控与计数", "Risk & counters") },
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
      <Card
        title={tr("持仓", "Positions")}
        icon={<Wallet className="h-4 w-4" />}
        className="xl:col-span-2"
        bodyClassName="p-0"
        extra={belief?.hedged ? <Badge>{tr("双向持仓", "Hedge mode")}</Badge> : undefined}
      >
        {s.positions.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-ink-faint">{tr("空仓", "Flat")}</p>
        ) : (
          <Table head={[tr("腿", "Leg"), tr("数量", "Qty"), tr("开仓均价", "Avg entry")]}>
            {s.positions.map((p) => (
              <tr key={p.side}>
                <td>
                  <Badge>{p.side === "LONG" ? tr("多", "Long") : p.side === "SHORT" ? tr("空", "Short") : p.side}</Badge>
                </td>
                <td className="font-mono">{p.amount.replace("-", "")}</td>
                <td className="font-mono">{entryOf(p.side)}</td>
              </tr>
            ))}
          </Table>
        )}
        <p className="border-t border-line px-4 py-2.5 text-xs text-ink-faint">
          {tr(
            `数量来自交易进程，均价来自它的 journal 重建（${journal.id ?? "—"}）。`,
            `Quantity comes from the trader; average entry is rebuilt from its journal (${journal.id ?? "—"}).`,
          )}
        </p>
      </Card>
      <Card
        title={tr("挂单", "Open orders")}
        icon={<ListOrdered className="h-4 w-4" />}
        className="xl:col-span-3"
        bodyClassName="p-0"
        extra={
          <span>
            {tr(`开仓 ${opening} · 平仓 ${list.length - opening}`, `Opening ${opening} · closing ${list.length - opening}`)}
            {s.limits ? tr(` · 上限 ${s.limits.max_working}`, ` · limit ${s.limits.max_working}`) : ""}
          </span>
        }
      >
        {orders.isError ? (
          <div className="p-4">
            <ErrorState error={orders.error} what={tr("挂单", "open orders")} />
          </div>
        ) : list.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-ink-faint">{tr("交易进程认为当前没有挂单", "The trader believes it has no open orders")}</p>
        ) : (
          <div className="max-h-[26rem] overflow-auto">
            <Table head={[tr("方向", "Side"), tr("价格", "Price"), tr("数量", "Qty"), tr("类型", "Type"), tr("客户端订单号", "Client order ID")]}>
              {[...list]
                .sort((a, b) => (b.price_ticks ?? 0) - (a.price_ticks ?? 0))
                .map((o) => (
                  <tr key={o.client_id}>
                    {/* Neutral: a sell is neither good nor bad (UI-BRIEF §8). */}
                    <td className="text-ink">{o.side === "BUY" ? tr("买", "Buy") : o.side === "SELL" ? tr("卖", "Sell") : "—"}</td>
                    <td className="font-mono">{scaled(o.price_ticks, s.price_scale)}</td>
                    <td className="font-mono">{scaled(o.qty_lots, s.qty_scale)}</td>
                    <td>
                      {o.closing ? (
                        <Badge tone="accent">{tr("平仓 · 停机保留", "Closing · kept on halt")}</Badge>
                      ) : (
                        <span className="text-ink-muted">{tr("开仓", "Opening")}</span>
                      )}
                    </td>
                    <td className="font-mono text-xs text-ink-faint">{o.client_id}</td>
                  </tr>
                ))}
            </Table>
          </div>
        )}
        <p className="border-t border-line px-4 py-2.5 text-xs text-ink-faint">
          {tr(
            "这是交易进程自己记的账；与交易所实际挂单的比对见「对账与归因」。",
            "These are the trader's own books; for a comparison with the venue's open orders, see Reconciliation & attribution.",
          )}
        </p>
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
      title={only === "fill" ? tr("成交", "Fills") : tr("订单事件", "Order events")}
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
              { value: "fill", label: tr("只看成交", "Fills only") },
              { value: "orders", label: tr("全部订单事件", "All order events") },
            ]}
          />
        </>
      }
    >
      {!journal.id ? (
        <div className="p-4">
          <Empty title={tr("还没有可读的 journal。", "No readable journal yet.")} next={tr("交易进程启动后会在 journal 目录写一份。", "The trader writes one into the journal directory when it starts.")} />
        </div>
      ) : q.isLoading ? (
        <div className="p-4">
          <Skeleton rows={5} />
        </div>
      ) : q.isError ? (
        <div className="p-4">
          <ErrorState error={q.error} what={tr("成交记录", "fill records")} />
        </div>
      ) : (
        <>
          <RecordRows page={q.data!} empty={only === "fill" ? tr("这次运行还没有成交。", "No fills in this run yet.") : tr("这次运行还没有订单事件。", "No order events in this run yet.")} />
          <div className="flex items-center justify-between border-t border-line px-4 py-2.5 text-xs text-ink-faint">
            <span>{tr(`共 ${q.data!.total} 条`, `${q.data!.total} total`)}</span>
            <div className="flex gap-2">
              {cursor !== null && (
                <Button size="sm" variant="ghost" onClick={() => setCursor(null)}>
                  {tr("回到最新", "Latest")}
                </Button>
              )}
              {q.data!.next_before !== null && (
                <Button size="sm" onClick={() => setCursor(q.data!.next_before)}>
                  {tr("更早", "Older")}
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
      <Table head={[tr("时间", "Time"), tr("类型", "Kind"), tr("内容", "Details"), tr("订单号", "Order ID")]} dense>
        {page.records.map((r) => (
          <tr key={r.seq}>
            <td className="whitespace-nowrap font-mono text-xs text-ink-muted">{r.at ? fmtTime(r.at / 1e6) : "—"}</td>
            <td className="whitespace-nowrap">
              <Badge tone={r.kind === "fill" ? "accent" : r.kind === "refused" ? "warn" : "neutral"}>{kindNames()[r.kind] ?? r.kind}</Badge>
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
  if (q.isError) return <ErrorState error={q.error} what={tr("行情", "market data")} />;
  const ticks = q.data?.records ?? [];
  if (!ticks.length) return (
      <Empty
        title={tr("这次运行的 journal 里还没有行情。", "No market data in this run's journal yet.")}
        next={tr("交易进程每个时间窗写一条 tick；刚启动时要等几秒。", "The trader writes one tick per time window; right after start, allow a few seconds.")}
      />
    );
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
          {tr(
            `这个价格已经 ${fmtDuration(ageS)}没更新——不是当前价。行情可能断了，看下方的计数和告警。`,
            `This price has not updated for ${fmtDuration(ageS)}; it is not the current price. The feed may be down; check the counters and alerts below.`,
          )}
        </div>
      )}
      <div className={cx("grid gap-4 sm:grid-cols-2 xl:grid-cols-4", stale && "opacity-60")}>
        <Stat label={tr("最新成交价", "Last trade")} value={px(latest.fields.last)} sub={agoText(ageS)} tone={stale ? "bad" : undefined} />
        <Stat label={tr("买一 / 卖一", "Bid / ask")} value={`${px(latest.fields.bid)} / ${px(latest.fields.ask)}`} />
        <Stat
          label={tr("到达延迟 p50 / p95", "Arrival latency p50 / p95")}
          value={stats ? tr(`${stats.p50.toFixed(2)} / ${stats.p95.toFixed(2)} 秒`, `${stats.p50.toFixed(2)} / ${stats.p95.toFixed(2)} s`) : "—"}
          sub={tr("交易所时间 → 进程收到", "Venue time → received by trader")}
        />
        <Stat
          label={tr("最长间隔", "Longest gap")}
          value={stats ? tr(`${stats.maxGap.toFixed(1)} 秒`, `${stats.maxGap.toFixed(1)} s`) : "—"}
          sub={stats ? tr(`超过 5 秒的间隔 ${stats.gaps5} 次`, `${stats.gaps5} gap${stats.gaps5 === 1 ? "" : "s"} over 5 s`) : undefined}
          tone={stats && stats.maxGap > 10 ? "warn" : undefined}
        />
      </div>
      <Card
        title={tr(
          `价格（最近 ${stats?.n ?? 0} 条，${stats ? fmtDuration(stats.span) : ""}）`,
          `Price (last ${stats?.n ?? 0} ticks, ${stats ? fmtDuration(stats.span) : ""})`,
        )}
      >
        {stats && <TimeSeries height={220} series={[{ name: "last", points: stats.series, area: true }]} />}
      </Card>
      <Card title={tr("行情计数（本次运行）", "Feed counters (this run)")}>
        <KV
          cols={3}
          items={[
            [tr("深度更新", "Depth updates"), s.feed.depth.toLocaleString()],
            [tr("逐笔成交", "Trades"), s.feed.trades.toLocaleString()],
            [tr("交易所时间倒退", "Venue time went backwards"), tr(`${s.feed.out_of_order.toLocaleString()} 次`, `${s.feed.out_of_order.toLocaleString()}×`)],
            [tr("静默窗口", "Quiet windows"), s.feed.quiet.toLocaleString()],
            [tr("快照", "Snapshots"), s.feed.snapshots.toLocaleString()],
            [<span key="r">{tr("重同步", "Resyncs")}</span>, <span key="v" className={s.feed.resyncs > 0 ? "text-warn" : ""}>{s.feed.resyncs}</span>],
            [<span key="u">{tr("读不出的消息", "Unreadable messages")}</span>, <span key="v" className={s.feed.unreadable > 0 ? "text-bad" : ""}>{s.feed.unreadable}</span>],
            [tr("本段时间戳倒退", "Timestamps backwards (window)"), stats ? tr(`${stats.backwards} 次`, `${stats.backwards}×`) : "—"],
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
      <Card title={tr("风控限额（生效中）", "Risk limits (in force)")} icon={<ShieldCheck className="h-4 w-4" />}>
        {!l ? (
          <p className="text-sm text-ink-muted">{tr("这个版本的交易进程不报告限额；发布新版本后显示。", "This trader version does not report its limits; they appear once a newer version is released.")}</p>
        ) : (
          <KV
            items={[
              [tr("单笔数量上限", "Max order qty"), lotsText(l.max_order_qty, s.qty_scale)],
              [tr("持仓上限", "Max position qty"), lotsText(l.max_position_qty, s.qty_scale)],
              [tr("单笔名义金额上限", "Max order notional"), l.max_order_notional],
              [tr("限价偏离标记价上限", "Max limit price deviation from mark"), `${(l.price_band_ppb / 1e7).toFixed(2)}%`],
              [tr("同时挂单上限", "Max open orders"), tr(`${l.max_working}（当前 ${s.resting}）`, `${l.max_working} (now ${s.resting})`)],
              [
                tr("发单频率上限", "Max order rate"),
                tr(`${l.max_rate} 单 / ${(l.rate_window_ns / 1e9).toFixed(0)} 秒`, `${l.max_rate} orders / ${(l.rate_window_ns / 1e9).toFixed(0)} s`),
              ],
            ]}
          />
        )}
      </Card>
      <Card title={tr("本次运行计数", "Run counters")}>
        <KV
          items={[
            [tr("发出订单", "Orders sent"), String(c.sent ?? 0)],
            [tr("成交", "Fills"), String(c.fills ?? 0)],
            [tr("连接断开", "Disconnects"), String(c.disconnects ?? 0)],
            [<span key="f">{tr("外来订单", "Foreign orders")}</span>, <span key="v" className={(c.foreign_orders ?? 0) > 0 ? "text-bad" : ""}>{c.foreign_orders ?? 0}</span>],
            [tr("无法入账的回报", "Unbookable reports"), String(c.unbookable_reports ?? 0)],
            [
              tr("进程自检", "Self-check"),
              <span key="v" className={s.reconcile.agreed === false ? "text-bad" : ""}>
                {s.reconcile.agreed === null ? tr("尚未核对", "Not checked yet") : s.reconcile.agreed ? tr("一致", "Agree") : tr("不一致", "Disagree")}
                {s.reconcile.at_ns ? tr(`（${fmtTime(s.reconcile.at_ns / 1e6, false)}）`, ` (${fmtTime(s.reconcile.at_ns / 1e6, false)})`) : ""}
                {tr(
                  ` · 累计不一致 ${s.reconcile.mismatches} · 读取失败 ${s.reconcile.unread}`,
                  ` · mismatches ${s.reconcile.mismatches} · read failures ${s.reconcile.unread}`,
                )}
              </span>,
            ],
          ]}
        />
      </Card>
      {Object.keys(s.waiting_on).length > 0 && (
        <Card title={tr("策略在等什么", "What the strategy is waiting on")} className="xl:col-span-2">
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
