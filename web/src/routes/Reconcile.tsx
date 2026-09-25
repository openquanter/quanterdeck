import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, CheckCircle2, ChevronDown, CircleHelp, ClipboardPaste, XCircle } from "lucide-react";

import { api, type JournalEntry, type LiveReconciliation } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { Badge, Button, Card, Help, PageHeader, TabBar, cx, fmtDuration, fmtTime, useTab } from "@/ui/kit";

import { Attribution } from "./Attribution";

const TABS = ["venue", "attribution"] as const;

/**
 * Does the process's account agree with the venue's, and where does the
 * live P&L part from the model's (docs/UI-V4 §4.4). Two questions about
 * the same run, one page.
 */
export function Reconcile() {
  const tab = useTab(TABS);
  return (
    <div>
      <PageHeader title="对账与归因" description="交易进程记下的账与交易所是否一致；实盘盈亏与模型盈亏的差额从哪里来。" />
      <TabBar
        tabs={[
          { key: "venue", label: "交易所对账" },
          { key: "attribution", label: "盈亏归因" },
        ]}
      />
      {tab === "venue" ? <VenueReconciliation /> : <Attribution embedded />}
    </div>
  );
}

function VenueReconciliation() {
  const journals = useQuery({ queryKey: ["journals"], queryFn: api.journals, refetchInterval: 60_000 });
  const sorted = [...(journals.data ?? [])].sort((a, b) => b.id.localeCompare(a.id));
  const newestId = sorted.find((j) => j.state === "read")?.id;
  const [id, setId] = useState<string | null>(null);
  useEffect(() => {
    if (!id && newestId) setId(newestId);
  }, [id, newestId]);
  const entry = sorted.find((j) => j.id === id);

  if (journals.isLoading) return <Skeleton tiles={3} rows={6} />;
  if (journals.isError) return <ErrorState error={journals.error} what="journal 列表" />;
  if (!sorted.length) return <Empty title="还没有 journal。" next="交易进程启动后会在 journal 目录里写下它；确认 OQ_DECK_JOURNALS_DIR 指向那个目录。" />;

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center gap-3">
        <label className="text-sm text-ink-muted">运行</label>
        <select
          className="h-8 rounded-md border border-line-strong bg-surface-raised px-2.5 font-mono text-xs text-ink"
          value={id ?? ""}
          onChange={(e) => setId(e.target.value)}
        >
          {sorted.map((j) => (
            <option key={j.id} value={j.id}>
              {j.id}
              {j.id === newestId ? "（进行中）" : ""}
              {j.state === "unreadable" ? "（读不了）" : ""}
            </option>
          ))}
        </select>
        <span className="text-xs text-ink-faint">只有进行中的运行能自动对账；更早的运行需要手动粘贴当时的交易所记录。</span>
      </div>
      {entry?.state === "unreadable" ? (
        <Card tone="bad">
          <p className="text-sm text-ink">这份 journal 读不了，里面的一切都不能拿来下结论。</p>
          <p className="mt-1 font-mono text-xs text-ink-muted">{entry.error}</p>
        </Card>
      ) : entry ? (
        <Reconciliation entry={entry} newest={entry.id === newestId} />
      ) : null}
    </div>
  );
}

function Reconciliation({ entry, newest }: { entry: JournalEntry; newest: boolean }) {
  const b = entry.belief!;
  const latest = useQuery({ queryKey: ["live", "latest"], queryFn: api.liveLatest, refetchInterval: 30_000, enabled: newest, retry: false });
  const auto = latest.data && latest.data.reconciliation.journal === entry.id ? latest.data : null;
  const [manual, setManual] = useState<LiveReconciliation["reconciliation"] | null>(null);

  const shown = manual ?? auto?.reconciliation ?? null;
  const ageMs = manual ? Date.now() - manual.venue.read_at_ms : (auto?.record_age_ms ?? 0);

  return (
    <div className="space-y-5">
      {b.undecodable > 0 && (
        <div className="flex items-center gap-2 rounded-[var(--radius-card)] border border-warn/40 bg-warn/8 px-4 py-3 text-sm text-ink">
          <AlertTriangle className="h-4 w-4 text-warn" />
          这份 journal 有 {b.undecodable} 条记录解不开。从有洞的 journal 重建出的结果，即使对上了也可能只是碰巧。
        </div>
      )}
      {newest && latest.isLoading && <Skeleton tiles={1} rows={2} />}
      {newest && latest.isError && <ErrorState error={latest.error} what="自动对账" />}
      {shown ? (
        <Verdict r={shown} ageMs={ageMs} source={manual ? "手动粘贴的记录" : "对账进程每分钟更新的最新读数"} />
      ) : (
        !newest && (
          <Card>
            <p className="text-sm text-ink-muted">这是一次已结束的运行，没有自动对账。展开下方「手动对账」，粘贴当时 oq-recon 记下的交易所记录。</p>
          </Card>
        )
      )}
      <div className="grid gap-5 lg:grid-cols-2">
        <Side
          title={
            <>
              进程认为（journal 重建）
              <Help term="belief" />
            </>
          }
          legs={b.legs.map(([leg, lots, ticks]) => [leg, (lots / 10 ** b.qty_scale).toFixed(b.qty_scale), (ticks / 10 ** b.price_scale).toFixed(b.price_scale)])}
          orders={shown?.believed.orders ?? b.resting}
          diff={shown?.venue.orders}
          foot={
            <>
              接管记录：{b.adopted ? "有" : <span className="text-warn">没有（「平」可能是真平，也可能是没记下）</span>}
              {b.hedged && " · 双向持仓：按腿分别显示，不显示净额"}
            </>
          }
        />
        <Side
          title="交易所实际"
          legs={shown ? shown.venue.legs.map(([leg, q, p]) => [leg, Number(q).toFixed(b.qty_scale), Number(p).toFixed(b.price_scale)]) : null}
          orders={shown?.venue.orders ?? null}
          diff={shown?.believed.orders}
          foot={shown ? `读于 ${fmtTime(shown.venue.read_at_ms)}` : "没有交易所读数"}
        />
      </div>
      <Manual journal={entry.id} onResult={setManual} />
    </div>
  );
}

function Verdict({ r, ageMs, source }: { r: LiveReconciliation["reconciliation"]; ageMs: number; source: string }) {
  const v = r.verdict;
  const stale = ageMs > 5 * 60_000;
  const tone = v === "agree" ? "good" : v === "disagree" ? "bad" : "warn";
  const Icon = v === "agree" ? CheckCircle2 : v === "disagree" ? XCircle : CircleHelp;
  return (
    <div
      className={cx(
        "rounded-[var(--radius-card)] border px-5 py-4",
        tone === "good" ? "border-good/30 bg-good/6" : tone === "bad" ? "border-bad/40 bg-bad/8" : "border-warn/40 bg-warn/8",
      )}
    >
      <div className="flex flex-wrap items-center gap-3">
        <Icon className={cx("h-6 w-6", tone === "good" ? "text-good" : tone === "bad" ? "text-bad" : "text-warn")} />
        <div className="text-lg font-semibold text-ink">{v === "agree" ? "一致" : v === "disagree" ? `不一致：${r.differences.length} 处差异` : "无法判断"}</div>
        <Badge tone={stale ? "warn" : "neutral"}>交易所读数 {fmtDuration(ageMs / 1000)}前</Badge>
        <span className="text-xs text-ink-faint">来源：{source}</span>
      </div>
      {v === "cannot_tell" && <p className="mt-2 text-sm text-ink-muted">没有差异，但 journal 不完整（有解不开的帧，或没有接管记录），不能算一致。</p>}
      {stale && <p className="mt-2 text-sm text-warn">读数超过 5 分钟：比较的是那时候的账户，不是现在。</p>}
      {r.differences.length > 0 && (
        <ul className="mt-3 space-y-1 text-sm text-ink">
          {r.differences.map((d, i) => (
            <li key={i} className="flex gap-2">
              <span className="text-bad">•</span>
              {d}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Side({
  title,
  legs,
  orders,
  diff,
  foot,
}: {
  title: React.ReactNode;
  legs: [string, string, string][] | null;
  orders: string[] | null;
  /** The other side's orders: those missing there are marked. */
  diff?: string[];
  foot: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const missing = (o: string) => diff !== undefined && !diff.includes(o);
  return (
    <Card title={title} bodyClassName="p-0">
      <div className="px-4 py-3">
        <div className="mb-1.5 text-xs text-ink-faint">持仓</div>
        {legs === null ? (
          <p className="text-sm text-ink-faint">—</p>
        ) : legs.length === 0 ? (
          <p className="text-sm text-ink">空仓</p>
        ) : (
          <ul className="space-y-1">
            {legs.map(([leg, qty, px]) => (
              <li key={leg} className="flex items-center gap-3 font-mono text-sm">
                <Badge>{leg}</Badge>
                <span className="text-ink">{qty}</span>
                <span className="text-ink-muted">@ {px}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="border-t border-line px-4 py-3">
        <button className="flex w-full items-center gap-2 text-xs text-ink-faint hover:text-ink" onClick={() => setOpen(!open)}>
          挂单 {orders?.length ?? "—"}
          {orders && diff && orders.some(missing) && <Badge tone="bad">{orders.filter(missing).length} 张对方没有</Badge>}
          <ChevronDown className={cx("ml-auto h-3.5 w-3.5 transition-transform", open && "rotate-180")} />
        </button>
        {open && orders && (
          <ul className="mt-2 max-h-48 space-y-0.5 overflow-auto font-mono text-xs">
            {orders.map((o) => (
              <li key={o} className={missing(o) ? "text-bad" : "text-ink-muted"}>
                {o}
                {missing(o) && " ← 对方没有"}
              </li>
            ))}
          </ul>
        )}
      </div>
      <p className="border-t border-line px-4 py-2.5 text-xs text-ink-faint">{foot}</p>
    </Card>
  );
}

function Manual({ journal, onResult }: { journal: string; onResult: (r: LiveReconciliation["reconciliation"] | null) => void }) {
  const [open, setOpen] = useState(false);
  const [pasted, setPasted] = useState("");
  const [error, setError] = useState<unknown>(null);
  return (
    <Card
      title="手动对账"
      icon={<ClipboardPaste className="h-4 w-4" />}
      extra={
        <Button size="sm" variant="ghost" onClick={() => setOpen(!open)}>
          {open ? "收起" : "展开"}
        </Button>
      }
      bodyClassName={open ? "p-4" : "hidden"}
    >
      <p className="text-xs text-ink-muted">
        控制台不持有交易所凭证。交易所那一侧只能来自 oq-recon 写下的记录：在有凭证的机器上运行
        <code className="mx-1 rounded bg-surface-raised px-1 font-mono">oq-recon BTCUSDT --record now.txt</code>
        ，把文件内容粘贴到这里。
      </p>
      <textarea
        className="mt-3 w-full rounded-md border border-line bg-ground p-2.5 font-mono text-xs text-ink"
        rows={6}
        value={pasted}
        onChange={(e) => setPasted(e.target.value)}
        placeholder={"# openquanter account record\nsymbol BTCUSDT\nread_at_ms …\nleg LONG 0.004 84435.5\norder …"}
      />
      <div className="mt-2 flex items-center gap-2">
        <Button
          variant="primary"
          disabled={!pasted.trim()}
          onClick={async () => {
            setError(null);
            try {
              onResult(await api.reconcile(journal, pasted));
            } catch (e) {
              onResult(null);
              setError(e);
            }
          }}
        >
          对账
        </Button>
        <Button variant="ghost" onClick={() => onResult(null)}>
          清除手动结果
        </Button>
      </div>
      {error ? (
        <div className="mt-3">
          <ErrorState error={error} what="手动对账" />
        </div>
      ) : null}
    </Card>
  );
}
