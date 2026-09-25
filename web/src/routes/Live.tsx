import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { api, type JournalEntry, type LiveReconciliation, type RecordsPage } from "@/api/client";
import { Empty, ErrorState, Expert, Skeleton, Term } from "@/components/States";

/**
 * What the process believes it holds, against what the venue holds
 * (UI-BRIEF §4.6), with the market it is trading in (§4.8) and the fills
 * it booked. Every number here comes from the process's own journal or
 * the venue reading oq-recon keeps — the console has no feed and no key
 * of its own, so a dead feed shows as dead.
 */
export function Live() {
  const journals = useQuery({ queryKey: ["journals"], queryFn: api.journals, refetchInterval: 60_000 });
  const readable = (journals.data ?? []).filter((j) => j.state === "read");
  const [id, setId] = useState<string | null>(null);
  useEffect(() => {
    // Newest first: the ids carry the start time.
    if (!id && readable.length) setId([...readable].sort((a, b) => b.id.localeCompare(a.id))[0].id);
  }, [readable, id]);
  const entry = (journals.data ?? []).find((j) => j.id === id);

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <h1 className="text-lg text-ink">实盘对账</h1>
        <select
          className="ml-auto rounded border border-line bg-ground p-1.5 font-mono text-xs"
          value={id ?? ""}
          onChange={(e) => setId(e.target.value)}
        >
          {[...(journals.data ?? [])]
            .sort((a, b) => b.id.localeCompare(a.id))
            .map((j) => (
              <option key={j.id} value={j.id}>
                {j.id}
                {j.state === "unreadable" ? "（读不了）" : ""}
              </option>
            ))}
        </select>
      </div>

      {journals.isLoading ? (
        <Skeleton tiles={4} rows={6} />
      ) : journals.isError ? (
        <ErrorState error={journals.error} what="journal 列表" />
      ) : !journals.data?.length ? (
        <Empty title="还没有 journal。" next="交易进程启动后会在 journal 目录里写下它；确认 OQ_DECK_JOURNALS_DIR 指向那个目录。" />
      ) : entry?.state === "unreadable" ? (
        <div className="rounded border border-bad/40 bg-bad/10 p-4 text-sm">
          <p className="text-ink">这份 journal 读不了，里面的一切都不能拿来下结论。</p>
          <p className="mt-1 font-mono text-xs text-ink-muted">{entry.error}</p>
        </div>
      ) : id && entry ? (
        <>
          <Market id={id} />
          <Reconciliation entry={entry} newest={id === [...readable].sort((a, b) => b.id.localeCompare(a.id))[0]?.id} />
          <Fills id={id} />
        </>
      ) : null}
    </div>
  );
}

// -- market ----------------------------------------------------------------

const STALE_S = 10;

function Market({ id }: { id: string }) {
  const q = useQuery({
    queryKey: ["records", id, "tick"],
    queryFn: () => api.records(id, ["tick"], 180),
    refetchInterval: 5_000,
  });
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  if (q.isLoading) return <Skeleton tiles={4} rows={0} />;
  if (q.isError) return <ErrorState error={q.error} what="行情" />;
  const ticks = q.data?.records ?? [];
  if (!ticks.length) {
    return <Empty title="这个 session 的 journal 里还没有行情。" next="交易进程每收盘一个时间窗写一条 tick；刚启动时要等几秒。若一直没有，看「运维」里的行情计数。" />;
  }
  const ps = q.data?.price_scale ?? 0;
  const px = (v: unknown) => (Number(v) / 10 ** ps).toFixed(ps);
  const latest = ticks[0];
  const seen = Number(latest.fields.seen);
  const at = latest.at ?? 0;
  const ageS = (now * 1e6 - seen) / 1e9;
  const skewS = (seen - at) / 1e9;
  const stale = ageS > STALE_S;
  const series = [...ticks].reverse().map((t) => Number(t.fields.last));

  return (
    <section>
      <h2 className="mb-2 text-sm text-ink-muted">行情（来自交易进程自己的 journal）</h2>
      {stale && (
        <div className="mb-2 rounded border border-bad bg-bad/15 px-3 py-2 text-sm text-ink">
          这个价格已经 {fmtAge(ageS)} 没更新了——不是当前价。交易进程的行情可能断了，去「运维」看行情计数与告警。
        </div>
      )}
      <div className={`grid gap-3 sm:grid-cols-5 ${stale ? "opacity-60" : ""}`}>
        <Tile label="last" value={px(latest.fields.last)} />
        <Tile label="bid" value={px(latest.fields.bid)} />
        <Tile label="ask" value={px(latest.fields.ask)} />
        <Tile label="距进程看到它" value={fmtAge(ageS)} tone={stale ? "bad" : undefined} />
        <Tile
          label="交易所时钟 → 本进程"
          value={`${skewS.toFixed(2)} 秒`}
          tone={Math.abs(skewS) > 2 ? "warn" : undefined}
          sub={Math.abs(skewS) > 2 ? "两个时钟在背离：数据在路上堵着，和价格不动是两种故障" : undefined}
        />
      </div>
      <Spark values={series} />
    </section>
  );
}

function fmtAge(s: number) {
  if (s < 0) return "0 秒";
  if (s < 90) return `${s.toFixed(0)} 秒`;
  if (s < 5400) return `${(s / 60).toFixed(0)} 分钟`;
  return `${(s / 3600).toFixed(1)} 小时`;
}

function Spark({ values }: { values: number[] }) {
  if (values.length < 2) return null;
  const lo = Math.min(...values), hi = Math.max(...values), span = hi - lo || 1;
  const W = 760, H = 60;
  const d = values.map((v, i) => `${i ? "L" : "M"}${(i / (values.length - 1)) * W},${H - ((v - lo) / span) * (H - 4) - 2}`).join(" ");
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="mt-2 h-16 w-full rounded border border-line bg-surface" aria-label="最近的 last">
      <path d={d} fill="none" stroke="var(--color-accent)" strokeWidth="1.5" />
    </svg>
  );
}

// -- reconciliation ----------------------------------------------------------

function Reconciliation({ entry, newest }: { entry: JournalEntry; newest: boolean }) {
  const b = entry.belief!;
  const latest = useQuery({ queryKey: ["live", "latest"], queryFn: api.liveLatest, refetchInterval: 30_000, enabled: newest, retry: false });
  const [pasted, setPasted] = useState("");
  const [manual, setManual] = useState<LiveReconciliation["reconciliation"] | null>(null);
  const [manualError, setManualError] = useState<unknown>(null);
  const ps = b.price_scale, qs = b.qty_scale;
  const auto = latest.data && latest.data.reconciliation.journal === entry.id ? latest.data : null;

  return (
    <section className="space-y-3">
      <h2 className="text-sm text-ink-muted">
        <Term name="belief">进程认为自己持有</Term> vs 交易所实际
      </h2>
      {b.undecodable > 0 && (
        <div className="rounded border border-warn bg-warn/10 px-3 py-2 text-sm text-ink">
          这份 journal 有 {b.undecodable} 条记录解不开。从有洞的 journal 重建出的结果，即使对上了也可能只是碰巧。
        </div>
      )}
      <div className="grid gap-3 sm:grid-cols-4">
        <Tile
          label="持仓"
          value={
            b.legs.length === 0
              ? "无"
              : b.legs.map(([leg, lots, entryTicks]) => `${leg} ${(lots / 10 ** qs).toFixed(qs)} @ ${(entryTicks / 10 ** ps).toFixed(ps)}`).join("  ")
          }
          sub={b.hedged ? "对冲账户：按两条腿分别显示，不显示净额" : undefined}
        />
        <Tile label="挂单" value={String(b.resting.length)} />
        <Tile label="接管的仓位" value={b.adopted ? "有记录" : "没有记录"} tone={b.adopted ? undefined : "warn"} sub={b.adopted ? undefined : "没有接管记录时，「平」可能是真的平，也可能是没记下来"} />
        <Tile label={<Term name="undecodable">解不开的记录</Term>} value={String(b.undecodable)} tone={b.undecodable ? "warn" : undefined} />
      </div>

      {newest && (
        <div>
          <h3 className="mb-1 text-xs text-ink-muted">自动对账（交易所最新读数，oq-recon 每分钟更新）</h3>
          {latest.isLoading ? <Skeleton rows={2} /> : latest.isError ? <ErrorState error={latest.error} what="自动对账" /> : auto ? <Verdict r={auto.reconciliation} ageMs={auto.record_age_ms} /> : null}
        </div>
      )}

      <details className="rounded border border-line p-3">
        <summary className="cursor-pointer text-sm text-ink">手动对账：粘贴一份 oq-recon --record 的输出</summary>
        <p className="mt-2 text-xs text-ink-muted">
          控制台不持有交易所凭证，也不会有。交易所那一侧只能来自 oq-recon 读到并写下的记录：在有凭证的机器上运行
          <code className="mx-1">oq-recon BTCUSDT --record now.txt</code>，把文件内容粘贴到这里。
        </p>
        <textarea
          className="mt-2 w-full rounded border border-line bg-ground p-2 font-mono text-xs"
          rows={6}
          value={pasted}
          onChange={(e) => setPasted(e.target.value)}
          placeholder={"# openquanter account record\nsymbol BTCUSDT\nread_at_ms …\nleg LONG 0.004 84435.5\norder …"}
        />
        <button
          className="mt-2 rounded border border-line px-3 py-1 text-xs text-ink hover:bg-surface-raised"
          disabled={!pasted.trim()}
          onClick={async () => {
            setManualError(null);
            try {
              setManual(await api.reconcile(entry.id, pasted));
            } catch (e) {
              setManual(null);
              setManualError(e);
            }
          }}
        >
          对账
        </button>
        {manualError ? <ErrorState error={manualError} what="手动对账" /> : null}
        {manual && <div className="mt-3"><Verdict r={manual} ageMs={Date.now() - manual.venue.read_at_ms} /></div>}
      </details>
    </section>
  );
}

function Verdict({ r, ageMs }: { r: LiveReconciliation["reconciliation"]; ageMs: number }) {
  const v = r.verdict;
  return (
    <div className="space-y-2">
      <div className="grid gap-3 sm:grid-cols-3">
        <Tile
          label="结论"
          value={v === "agree" ? "一致" : v === "disagree" ? "不一致" : "无法判断"}
          tone={v === "agree" ? "good" : v === "disagree" ? "bad" : "warn"}
          sub={v === "cannot_tell" ? "没有差异，但 journal 不完整（有解不开的帧，或没有接管记录），不能算一致" : undefined}
        />
        <Tile label="交易所读数" value={`${fmtAge(ageMs / 1000)}前`} tone={ageMs > 5 * 60_000 ? "warn" : undefined} sub={ageMs > 5 * 60_000 ? "读数过旧：比较的是那时候的账户" : undefined} />
        <Tile label="差异" value={String(r.differences.length)} />
      </div>
      {r.differences.length > 0 && (
        <ul className="space-y-1 rounded border border-bad/50 bg-bad/10 p-3 text-sm">
          {r.differences.map((d, i) => (
            <li key={i}>{d}</li>
          ))}
        </ul>
      )}
      <Expert>
        <div className="grid gap-3 text-xs sm:grid-cols-2">
          <Side title="进程认为（journal 重建）" rec={r.believed} />
          <Side title="交易所实际" rec={r.venue} />
        </div>
      </Expert>
    </div>
  );
}

function Side({ title, rec }: { title: string; rec: { legs: [string, number, number][]; orders: string[] } }) {
  return (
    <div className="rounded border border-line bg-surface p-3">
      <div className="mb-1 text-ink-muted">{title}</div>
      {rec.legs.length === 0 ? <div>无持仓</div> : rec.legs.map(([s, q, p]) => <div key={s} className="font-mono">{s} {q} @ {p}</div>)}
      <div className="mt-1 text-ink-muted">挂单 {rec.orders.length}</div>
      <ul className="max-h-32 overflow-auto font-mono text-ink-muted">
        {rec.orders.map((o) => <li key={o}>{o}</li>)}
      </ul>
    </div>
  );
}

// -- fills -----------------------------------------------------------------

const ORDER_KINDS = ["fill", "submitted", "cancelled", "outcome", "refused", "operator"];

function Fills({ id }: { id: string }) {
  const [only, setOnly] = useState<"fill" | "all">("fill");
  const q = useQuery({
    queryKey: ["records", id, only],
    queryFn: () => api.records(id, only === "fill" ? ["fill"] : ORDER_KINDS, 100),
    refetchInterval: 10_000,
  });
  return (
    <section>
      <div className="mb-2 flex items-center gap-3">
        <h2 className="text-sm text-ink-muted">成交流水</h2>
        <select className="rounded border border-line bg-ground p-1 text-xs" value={only} onChange={(e) => setOnly(e.target.value as "fill" | "all")}>
          <option value="fill">只看成交</option>
          <option value="all">订单全过程</option>
        </select>
        {q.data && <span className="text-xs text-ink-muted">共 {q.data.total} 条</span>}
      </div>
      {q.isLoading ? <Skeleton rows={4} /> : q.isError ? <ErrorState error={q.error} what="成交流水" /> : <RecordTable page={q.data!} emptyNext="这次运行还没有成交。" />}
    </section>
  );
}

export function RecordTable({ page, emptyNext }: { page: RecordsPage; emptyNext: string }) {
  const rows = page.records;
  const ps = page.price_scale, qs = page.qty_scale;
  const scale = (v: unknown, s: number) => (typeof v === "number" ? (v / 10 ** s).toFixed(s) : String(v));
  const summary = useMemo(
    () => (r: RecordsPage["records"][number]) => {
      const f = r.fields;
      switch (r.kind) {
        case "fill": return `${f.side} ${f.qty} @ ${f.price} · ${f.client_id}`;
        case "submitted": return `${f.side} ${scale(f.qty, qs)} @ ${f.limit_price === 0 ? "市价" : scale(f.limit_price, ps)}${f.leg ? ` ${f.leg}` : ""}${f.reduce_only ? " 平仓" : ""} · ${f.client_id}`;
        case "outcome": return `${f.tag} ${f.detail} · ${f.client_id}`;
        case "cancelled": return `撤单 · ${f.client_id}`;
        case "refused": return `风控拒绝：${f.breach}`;
        case "operator": return `${f.command}：${f.reason} → ${f.outcome}（${f.origin}）`;
        case "tick": return `last ${scale(f.last, ps)} bid ${scale(f.bid, ps)} ask ${scale(f.ask, ps)}`;
        case "reconciled": return `接管 ${JSON.stringify(f.legs)}`;
        case "waiting": return Object.entries(f).map(([k, v]) => `${k} ${v}`).join(", ");
        default: return JSON.stringify(f);
      }
    },
    [ps, qs],
  );
  if (!rows.length) return <Empty title="没有记录。" next={emptyNext} />;
  return (
    <div className="max-h-[28rem] overflow-auto rounded border border-line">
      <table className="w-full text-sm">
        <thead className="sticky top-0 bg-surface text-left text-xs text-ink-muted">
          <tr>
            <th className="px-2 py-1 font-normal">#</th>
            <th className="px-2 py-1 font-normal">时间</th>
            <th className="px-2 py-1 font-normal">类型</th>
            <th className="px-2 py-1 font-normal">内容</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.seq} className="border-t border-line">
              <td className="px-2 py-1 font-mono text-xs text-ink-muted">{r.seq}</td>
              <td className="px-2 py-1 font-mono text-xs text-ink-muted">{r.at ? new Date(r.at / 1e6).toLocaleString("zh-CN", { hour12: false }) : "—"}</td>
              <td className="px-2 py-1 text-xs">{KIND_NAMES[r.kind] ?? r.kind}</td>
              {/* Text, never markup: these fields come from a venue. */}
              <td className="px-2 py-1 font-mono text-xs">{summary(r)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export const KIND_NAMES: Record<string, string> = {
  session_start: "启动",
  tick: "行情",
  submitted: "下单",
  outcome: "回报",
  cancelled: "撤单",
  fill: "成交",
  refused: "风控拒绝",
  reconciled: "接管仓位",
  waiting: "策略等待",
  operator: "操作者",
};

function Tile({ label, value, sub, tone }: { label: React.ReactNode; value: string; sub?: string; tone?: "good" | "bad" | "warn" }) {
  const color = tone === "bad" ? "text-bad" : tone === "good" ? "text-good" : tone === "warn" ? "text-warn" : "text-ink";
  const border = tone === "warn" ? "border-warn/60" : "border-line";
  return (
    <div className={`rounded border ${border} bg-surface p-3`}>
      <div className="text-xs text-ink-muted">{label}</div>
      <div className={`mt-1 font-mono text-base tabular-nums ${color}`}>{value}</div>
      {sub && <div className="mt-1 text-xs text-ink-muted">{sub}</div>}
    </div>
  );
}
