import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, CheckCircle2, ChevronDown, CircleHelp, ClipboardPaste, XCircle } from "lucide-react";

import { api, type JournalEntry, type LiveReconciliation } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { tr } from "@/i18n";
import { Badge, Button, Card, Help, PageHeader, TabBar, agoText, cx, fmtTime, useTab } from "@/ui/kit";

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
      <PageHeader
        title={tr("对账与归因", "Reconciliation & attribution")}
        description={tr(
          "交易进程记下的账与交易所是否一致；实盘盈亏与模型盈亏的差额从哪里来。",
          "Whether the trader's books agree with the venue's, and where live P&L parts from the model's.",
        )}
      />
      <TabBar
        tabs={[
          { key: "venue", label: tr("交易所对账", "Venue reconciliation") },
          { key: "attribution", label: tr("盈亏归因", "P&L attribution") },
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
  if (journals.isError) return <ErrorState error={journals.error} what={tr("journal 列表", "the journal list")} />;
  if (!sorted.length)
    return (
      <Empty
        title={tr("还没有 journal。", "No journals yet.")}
        next={tr(
          "交易进程启动后会在 journal 目录里写下它；确认 OQ_DECK_JOURNALS_DIR 指向那个目录。",
          "The trader writes one into the journal directory when it starts; check that OQ_DECK_JOURNALS_DIR points at that directory.",
        )}
      />
    );

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center gap-3">
        <label className="text-sm text-ink-muted">{tr("运行", "Run")}</label>
        <select
          className="h-8 rounded-md border border-line-strong bg-surface-raised px-2.5 font-mono text-xs text-ink"
          value={id ?? ""}
          onChange={(e) => setId(e.target.value)}
        >
          {sorted.map((j) => (
            <option key={j.id} value={j.id}>
              {j.id}
              {j.id === newestId ? tr("（进行中）", " (running)") : ""}
              {j.state === "unreadable" ? tr("（读不了）", " (unreadable)") : ""}
            </option>
          ))}
        </select>
        <span className="text-xs text-ink-faint">
          {tr(
            "只有进行中的运行能自动对账；更早的运行需要手动粘贴当时的交易所记录。",
            "Only the current run reconciles automatically; for earlier runs, paste the venue record from that time.",
          )}
        </span>
      </div>
      {entry?.state === "unreadable" ? (
        <Card tone="bad">
          <p className="text-sm text-ink">{tr("这份 journal 读不了，里面的一切都不能拿来下结论。", "This journal cannot be read; nothing in it can be relied on.")}</p>
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
          {tr(
            `这份 journal 有 ${b.undecodable} 条记录解不开。从有洞的 journal 重建出的结果，即使对上了也可能只是碰巧。`,
            `${b.undecodable} record${b.undecodable === 1 ? "" : "s"} in this journal could not be decoded. What is rebuilt from a journal with holes may agree by luck alone.`,
          )}
        </div>
      )}
      {newest && latest.isLoading && <Skeleton tiles={1} rows={2} />}
      {newest && latest.isError && <ErrorState error={latest.error} what={tr("自动对账", "automatic reconciliation")} />}
      {shown ? (
        <Verdict r={shown} ageMs={ageMs} source={manual ? tr("手动粘贴的记录", "a pasted record") : tr("对账进程每分钟更新的最新读数", "the latest reading, refreshed every minute by the reconciler")} />
      ) : (
        !newest && (
          <Card>
            <p className="text-sm text-ink-muted">
              {tr(
                "这是一次已结束的运行，没有自动对账。展开下方「手动对账」，粘贴当时 oq-recon 记下的交易所记录。",
                "This run has ended, so there is no automatic reconciliation. Open \"Manual reconciliation\" below and paste the venue record oq-recon wrote at the time.",
              )}
            </p>
          </Card>
        )
      )}
      <div className="grid gap-5 lg:grid-cols-2">
        <Side
          title={
            <>
              {tr("进程认为（journal 重建）", "Trader's view (rebuilt from journal)")}
              <Help term="belief" />
            </>
          }
          legs={b.legs.map(([leg, lots, ticks]) => [leg, (lots / 10 ** b.qty_scale).toFixed(b.qty_scale), (ticks / 10 ** b.price_scale).toFixed(b.price_scale)])}
          orders={shown?.believed.orders ?? b.resting}
          diff={shown?.venue.orders}
          foot={
            <>
              {tr("接管记录：", "Adoption record: ")}
              {b.adopted ? (
                tr("有", "yes")
              ) : (
                <span className="text-warn">{tr("没有（「平」可能是真平，也可能是没记下）", "none (\"flat\" may be truly flat, or just unrecorded)")}</span>
              )}
              {b.hedged && tr(" · 双向持仓：按腿分别显示，不显示净额", " · Hedge mode: shown per leg, not netted")}
            </>
          }
        />
        <Side
          title={tr("交易所实际", "Venue actual")}
          legs={shown ? shown.venue.legs.map(([leg, q, p]) => [leg, Number(q).toFixed(b.qty_scale), Number(p).toFixed(b.price_scale)]) : null}
          orders={shown?.venue.orders ?? null}
          diff={shown?.believed.orders}
          foot={shown ? tr(`读于 ${fmtTime(shown.venue.read_at_ms)}`, `Read at ${fmtTime(shown.venue.read_at_ms)}`) : tr("没有交易所读数", "No venue reading")}
        />
      </div>
      <Manual journal={entry.id} onResult={setManual} />
    </div>
  );
}

/**
 * Why the two could not be compared.
 *
 * The core hands back which fact stopped it rather than a sentence, so
 * the sentence is worded here, once, in both languages — and a case
 * added there cannot arrive as a blank.
 */
function cannotTell(why: LiveReconciliation["reconciliation"]["cannot_tell"]) {
  switch (why) {
    case "undecodable":
      return tr(
        "journal 里有解不开的帧，重建出来的账可能只是碰巧对上。",
        "Frames in the journal did not decode, so what was rebuilt may agree by luck.",
      );
    case "no_adoption":
      return tr(
        "journal 里没有接管记录：读出来是空仓，既可能是真空仓，也可能是持着没人写下来的仓位。",
        "No adoption record: a flat reconstruction means flat, or means a position nobody wrote down.",
      );
    case "reading_predates_the_run":
      return tr(
        "这份交易所读数是本轮启动之前取的，它描述的是上一轮——两轮的挂单号不同，逐条比下去每一条都会「不一致」。读数每分钟重写一次，等它追上即可。",
        "This venue reading was taken before the current run started, so it describes the run before it. The two runs' orders differ, and comparing them line by line reports every one of them as a difference. The reading is rewritten about once a minute; wait for it to catch up.",
      );
    default:
      return tr(
        "没有差异，但 journal 不完整，不能算一致。",
        "No differences, but the journal is incomplete, so this does not count as agreement.",
      );
  }
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
        <div className="text-lg font-semibold text-ink">{v === "agree"
            ? tr("一致", "Agree")
            : v === "disagree"
              ? tr(`不一致：${r.differences.length} 处差异`, `Disagree: ${r.differences.length} difference${r.differences.length === 1 ? "" : "s"}`)
              : tr("无法判断", "Cannot tell")}</div>
        <Badge tone={stale ? "warn" : "neutral"}>{tr(`交易所读数 ${agoText(ageMs / 1000)}`, `Venue read ${agoText(ageMs / 1000)}`)}</Badge>
        <span className="text-xs text-ink-faint">{tr(`来源：${source}`, `Source: ${source}`)}</span>
      </div>
      {v === "cannot_tell" && (
        <p className="mt-2 text-sm text-ink-muted">{cannotTell(r.cannot_tell)}</p>
      )}
      {stale && (
        <p className="mt-2 text-sm text-warn">
          {tr("读数超过 5 分钟：比较的是那时候的账户，不是现在。", "The reading is over 5 minutes old: this compares the account as it was then, not now.")}
        </p>
      )}
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
        <div className="mb-1.5 text-xs text-ink-faint">{tr("持仓", "Positions")}</div>
        {legs === null ? (
          <p className="text-sm text-ink-faint">—</p>
        ) : legs.length === 0 ? (
          <p className="text-sm text-ink">{tr("空仓", "Flat")}</p>
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
          {tr("挂单", "Open orders")} {orders?.length ?? "—"}
          {orders && diff && orders.some(missing) && <Badge tone="bad">{tr(`${orders.filter(missing).length} 张对方没有`, `${orders.filter(missing).length} missing on the other side`)}</Badge>}
          <ChevronDown className={cx("ml-auto h-3.5 w-3.5 transition-transform", open && "rotate-180")} />
        </button>
        {open && orders && (
          <ul className="mt-2 max-h-48 space-y-0.5 overflow-auto font-mono text-xs">
            {orders.map((o) => (
              <li key={o} className={missing(o) ? "text-bad" : "text-ink-muted"}>
                {o}
                {missing(o) && tr(" ← 对方没有", " ← missing on the other side")}
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
      title={tr("手动对账", "Manual reconciliation")}
      icon={<ClipboardPaste className="h-4 w-4" />}
      extra={
        <Button size="sm" variant="ghost" onClick={() => setOpen(!open)}>
          {open ? tr("收起", "Collapse") : tr("展开", "Expand")}
        </Button>
      }
      bodyClassName={open ? "p-4" : "hidden"}
    >
      <p className="text-xs text-ink-muted">
        {tr(
          "控制台不持有交易所凭证。交易所那一侧只能来自 oq-recon 写下的记录：在有凭证的机器上运行",
          "The deck holds no venue credentials. The venue side can only come from a record oq-recon writes: on a machine that has credentials, run",
        )}
        <code className="mx-1 rounded bg-surface-raised px-1 font-mono">oq-recon BTCUSDT --record now.txt</code>
        {tr("，把文件内容粘贴到这里。", "and paste the file's contents here.")}
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
          {tr("对账", "Reconcile")}
        </Button>
        <Button variant="ghost" onClick={() => onResult(null)}>
          {tr("清除手动结果", "Clear manual result")}
        </Button>
      </div>
      {error ? (
        <div className="mt-3">
          <ErrorState error={error} what={tr("手动对账", "manual reconciliation")} />
        </div>
      ) : null}
    </Card>
  );
}
