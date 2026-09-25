import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, ScrollText } from "lucide-react";

import { api } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { kindNames } from "@/features/trading";
import { Button, Card, Help, PageHeader, cx } from "@/ui/kit";

import { RecordRows } from "./Trading";

/** What a replay shows before anything is chosen: every decision, not the market data. */
const DEFAULT_KINDS = ["submitted", "outcome", "fill", "cancelled", "refused", "operator", "reconciled", "session_start"];

/**
 * A run's decisions in the order they happened (事件回放, docs/UI-V4
 * §3): every record the process wrote before acting, newest first,
 * filtered by kind, a page at a time.
 */
export function Journal() {
  const journals = useQuery({ queryKey: ["journals"], queryFn: api.journals });
  const ids = useMemo(() => (journals.data ?? []).map((j) => j.id).sort().reverse(), [journals.data]);
  const [id, setId] = useState<string | null>(null);
  const [kinds, setKinds] = useState<string[]>(DEFAULT_KINDS);
  // Cursors of the pages stepped back through; empty is the newest page.
  const [before, setBefore] = useState<number[]>([]);
  useEffect(() => {
    if (!id && ids.length) setId(ids[0]);
  }, [ids, id]);
  useEffect(() => setBefore([]), [id, kinds]);

  const cursor = before[before.length - 1] ?? null;
  const page = useQuery({
    queryKey: ["records", id, kinds.join(","), cursor],
    queryFn: () => api.records(id as string, kinds, 200, cursor),
    enabled: id !== null && kinds.length > 0,
  });
  const entry = (journals.data ?? []).find((j) => j.id === id);

  const toggle = (k: string) => setKinds(kinds.includes(k) ? kinds.filter((x) => x !== k) : [...kinds, k]);

  return (
    <div className="space-y-5">
      <PageHeader
        title={
          <span className="inline-flex items-center gap-2">
            事件回放 <Help term="journal" />
          </span>
        }
        description="交易进程在行动前写下的每一条记录，最新的在前；按类型筛选，一页 200 条。"
        actions={
          ids.length > 0 && (
            <select
              className="h-8 rounded-md border border-line-strong bg-surface-raised px-2.5 font-mono text-xs text-ink"
              value={id ?? ""}
              onChange={(e) => setId(e.target.value)}
            >
              {ids.map((j, k) => {
                const e = journals.data!.find((x) => x.id === j);
                return (
                  <option key={j} value={j}>
                    {j}
                    {k === 0 ? "（最新）" : ""}
                    {e?.state === "unreadable" ? "（读不了）" : ""}
                  </option>
                );
              })}
            </select>
          )
        }
      />

      <div className="flex flex-wrap items-center gap-1.5">
        <span className="mr-1 text-xs text-ink-muted">类型</span>
        {Object.entries(kindNames()).map(([k, name]) => (
          <button
            key={k}
            onClick={() => toggle(k)}
            className={cx(
              "h-7 rounded-full px-2.5 text-xs ring-1 ring-inset transition-colors",
              kinds.includes(k) ? "bg-accent/12 text-accent ring-accent/40" : "text-ink-muted ring-line-strong hover:text-ink",
            )}
          >
            {name}
          </button>
        ))}
        <Button size="sm" variant="ghost" onClick={() => setKinds(DEFAULT_KINDS)}>
          恢复默认
        </Button>
      </div>

      {journals.isLoading ? (
        <Skeleton rows={8} />
      ) : journals.isError ? (
        <ErrorState error={journals.error} what="journal 列表" />
      ) : !journals.data?.length ? (
        <Empty title="还没有 journal。" next="交易进程启动后会写下它；确认 OQ_DECK_JOURNALS_DIR 指向那个目录。" />
      ) : entry?.state === "unreadable" ? (
        <Card tone="bad">
          <p className="text-sm text-ink">这份 journal 读不了。</p>
          {entry.error && <p className="mt-1 font-mono text-xs text-ink-muted">{entry.error}</p>}
        </Card>
      ) : kinds.length === 0 ? (
        <Empty title="没有选任何记录类型。" next="在上面点选要看的类型。" />
      ) : page.isLoading ? (
        <Skeleton rows={8} />
      ) : page.isError ? (
        <ErrorState error={page.error} what="journal 记录" />
      ) : page.data ? (
        <div className="space-y-3">
          {page.data.undecodable > 0 && (
            <div className="flex items-center gap-2 rounded-[var(--radius-card)] border border-warn/40 bg-warn/8 px-4 py-3 text-sm text-ink">
              <AlertTriangle className="h-4 w-4 text-warn" />
              这份 journal 有 {page.data.undecodable} 条记录解不开，回放有洞。
            </div>
          )}
          <Card
            title="记录"
            icon={<ScrollText className="h-4 w-4" />}
            bodyClassName="p-0"
            extra={<span className="font-mono">{page.data.journal}</span>}
          >
            <RecordRows page={page.data} empty="这份 journal 里没有所选类型的记录。" />
            <div className="flex items-center justify-between border-t border-line px-4 py-2.5 text-xs text-ink-faint">
              <span>
                共 {page.data.total} 条所选类型的记录{before.length > 0 && ` · 往前第 ${before.length + 1} 页`}
              </span>
              <div className="flex gap-2">
                {before.length > 0 && (
                  <>
                    <Button size="sm" variant="ghost" onClick={() => setBefore([])}>
                      回到最新
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setBefore(before.slice(0, -1))}>
                      较新
                    </Button>
                  </>
                )}
                <Button size="sm" disabled={!page.data.next_before} onClick={() => setBefore([...before, page.data!.next_before!])}>
                  较早
                </Button>
              </div>
            </div>
          </Card>
        </div>
      ) : null}
    </div>
  );
}
