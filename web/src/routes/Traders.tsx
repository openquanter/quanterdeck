import { useQuery } from "@tanstack/react-query";
import { Layers } from "lucide-react";

import { api, type TraderEntry, type TraderStatus } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { pair, tr } from "@/i18n";
import { Badge, Card, KV, Money, PageHeader, StatusDot, Freshness } from "@/ui/kit";

/** Traders grouped by what they run: strategy, then symbol. */
function grouped(entries: TraderEntry[]): [string, [string, TraderEntry[]][]][] {
  const by = new Map<string, Map<string, TraderEntry[]>>();
  for (const e of entries) {
    const strategy = "status" in e ? e.status.strategy || "—" : "—";
    const symbol = "status" in e ? e.status.symbol || "—" : "—";
    const s = by.get(strategy) ?? new Map<string, TraderEntry[]>();
    s.set(symbol, [...(s.get(symbol) ?? []), e]);
    by.set(strategy, s);
  }
  return [...by.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([strategy, symbols]) => [strategy, [...symbols.entries()].sort(([a], [b]) => a.localeCompare(b))]);
}

function position(t: TraderStatus): string {
  return (
    t.positions
      .map((p) => `${p.side === "LONG" ? tr("多", "Long") : p.side === "SHORT" ? tr("空", "Short") : p.side} ${p.amount.replace("-", "")}`)
      .join(" · ") || tr("无", "None")
  );
}

function TraderCard({ entry }: { entry: TraderEntry }) {
  if (!("status" in entry)) {
    return (
      <Card title={<span className="font-mono">{entry.id}</span>}>
        <div className="flex items-center gap-2 text-sm text-warn">
          <StatusDot tone="warn" />
          {pair(entry.error, entry.error_en)}
        </div>
      </Card>
    );
  }
  const t = entry.status;
  const feedBad = t.feed.unreadable > 0 || t.feed.resyncs > 0;
  return (
    <Card
      title={<span className="font-mono">{entry.id}</span>}
      extra={
        t.halted ? (
          <Badge tone="bad" dot>
            {tr("已停机", "Halted")}
          </Badge>
        ) : (
          <Badge tone="good" dot>
            {tr("交易中", "Trading")}
          </Badge>
        )
      }
    >
      <KV
        cols={2}
        items={[
          [tr("部署", "Deployment"), `${t.deployment} · pid ${t.pid}`],
          [tr("持仓", "Positions"), position(t)],
          [tr("挂单", "Open orders"), String(t.resting)],
          [tr("权益", "Equity"), t.pnl ? <Money key="e" value={t.pnl.equity} /> : tr("未报告", "not reported")],
          [
            tr("本次运行净盈亏", "Run net P&L"),
            // Unknown fees make the net unknown, not zero.
            t.pnl?.net != null ? <Money key="n" value={t.pnl.net} signed /> : tr("未测得", "not measured"),
          ],
          [
            tr("持仓核对", "Position check"),
            <span
              key="r"
              className={t.reconcile.agreed === false ? "text-bad" : t.reconcile.agreed == null ? "text-warn" : "text-good"}
            >
              {t.reconcile.agreed === true
                ? tr("与交易所一致", "Agrees with the venue")
                : t.reconcile.agreed === false
                  ? tr(`不一致（累计 ${t.reconcile.mismatches} 次）`, `Disagrees (${t.reconcile.mismatches} so far)`)
                  : tr("尚未核对", "Not yet checked")}
            </span>,
          ],
          [
            tr("行情", "Feed"),
            <span key="f" className={feedBad ? "text-warn" : ""}>
              {tr(
                `读不出 ${t.feed.unreadable} · 重同步 ${t.feed.resyncs}`,
                `${t.feed.unreadable} unreadable · ${t.feed.resyncs} resyncs`,
              )}
            </span>,
          ],
        ]}
      />
      {t.halted && t.halt_reason && <p className="mt-3 text-sm text-bad">{tr(`停机原因：${t.halt_reason}`, `Halted because: ${t.halt_reason}`)}</p>}
      {t.journal_lost && <p className="mt-2 text-sm text-bad">{tr(`日志无法写入：${t.journal_lost}`, `Journal cannot be written: ${t.journal_lost}`)}</p>}
    </Card>
  );
}

/**
 * Every trader on the host, grouped by strategy and symbol. The agent
 * finds them by their control sockets; one that does not answer is shown
 * with why, never left out — a missing card would read as "not running".
 */
export function Traders() {
  const q = useQuery({ queryKey: ["ops", "traders"], queryFn: api.traders, refetchInterval: 15_000 });
  const entries = q.data ?? [];
  const halted = entries.filter((e) => "status" in e && e.status.halted).length;
  const silent = entries.filter((e) => !("status" in e)).length;

  return (
    <div className="space-y-5">
      <PageHeader
        title={tr("交易进程", "Traders")}
        description={tr(
          "主机上每个交易进程，按策略和品种分组；由各自的控制端口找到。",
          "Every trader on the host, grouped by strategy and symbol; each found by its control socket.",
        )}
        meta={<Freshness at={q.dataUpdatedAt} fetching={q.isFetching} staleAfterS={45} onRefresh={() => void q.refetch()} />}
      />
      {q.isLoading ? (
        <Skeleton tiles={2} rows={3} />
      ) : q.isError ? (
        <ErrorState error={q.error} what={tr("交易进程", "traders")} />
      ) : entries.length === 0 ? (
        <Empty
          title={tr("没有找到交易进程的控制端口。", "No trader control socket was found.")}
          next={tr("交易进程启动时在控制目录里创建它的端口。", "A trader creates its socket in the control directory when it starts.")}
        />
      ) : (
        <>
          <p className="text-sm text-ink-muted">
            {tr(
              `${entries.length} 个交易进程 · ${halted} 个已停机 · ${silent} 个无应答`,
              `${entries.length} trader(s) · ${halted} halted · ${silent} not answering`,
            )}
          </p>
          {grouped(entries).map(([strategy, symbols]) => (
            <section key={strategy} className="space-y-3">
              <h2 className="flex items-center gap-2 text-base font-semibold text-ink">
                <Layers className="h-4 w-4" />
                {strategy}
              </h2>
              {symbols.map(([symbol, list]) => (
                <div key={symbol} className="space-y-2">
                  <div className="text-xs font-medium uppercase tracking-wide text-ink-faint">{symbol}</div>
                  <div className="grid gap-3 lg:grid-cols-2">
                    {list.map((e) => (
                      <TraderCard key={e.id} entry={e} />
                    ))}
                  </div>
                </div>
              ))}
            </section>
          ))}
        </>
      )}
    </div>
  );
}
