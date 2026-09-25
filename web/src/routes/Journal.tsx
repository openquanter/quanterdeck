import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { api } from "@/api/client";
import { Empty, ErrorState, Skeleton, Term } from "@/components/States";

import { KIND_NAMES, RecordTable } from "./Live";

/**
 * A run's decisions in the order they happened (UI-BRIEF §4.7): every
 * record the process wrote before acting, newest first, filtered by
 * kind, a page at a time.
 */
export function Journal() {
  const journals = useQuery({ queryKey: ["journals"], queryFn: api.journals });
  const [id, setId] = useState<string | null>(null);
  const [kinds, setKinds] = useState<string[]>(["submitted", "outcome", "fill", "cancelled", "refused", "operator", "reconciled", "session_start"]);
  const [before, setBefore] = useState<number[]>([]);
  useEffect(() => {
    const ids = (journals.data ?? []).map((j) => j.id).sort().reverse();
    if (!id && ids.length) setId(ids[0]);
  }, [journals.data, id]);
  useEffect(() => setBefore([]), [id, kinds]);

  const cursor = before[before.length - 1] ?? null;
  const page = useQuery({
    queryKey: ["records", id, kinds.join(","), cursor],
    queryFn: () => api.records(id as string, kinds, 200, cursor),
    enabled: id !== null && kinds.length > 0,
  });

  const toggle = (k: string) => setKinds(kinds.includes(k) ? kinds.filter((x) => x !== k) : [...kinds, k]);

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <h1 className="text-lg text-ink">
          <Term name="journal">Journal</Term> 回放
        </h1>
        <select className="ml-auto rounded border border-line bg-ground p-1.5 font-mono text-xs" value={id ?? ""} onChange={(e) => setId(e.target.value)}>
          {(journals.data ?? []).map((j) => j.id).sort().reverse().map((j) => (
            <option key={j}>{j}</option>
          ))}
        </select>
      </div>
      <div className="flex flex-wrap gap-2">
        {Object.entries(KIND_NAMES).map(([k, name]) => (
          <button
            key={k}
            onClick={() => toggle(k)}
            className={`rounded border px-2 py-0.5 text-xs ${kinds.includes(k) ? "border-accent text-accent" : "border-line text-ink-muted"}`}
          >
            {name}
          </button>
        ))}
      </div>
      {journals.isLoading ? (
        <Skeleton rows={8} />
      ) : journals.isError ? (
        <ErrorState error={journals.error} what="journal 列表" />
      ) : !journals.data?.length ? (
        <Empty title="还没有 journal。" next="交易进程启动后会写下它；确认 OQ_DECK_JOURNALS_DIR 指向那个目录。" />
      ) : kinds.length === 0 ? (
        <Empty title="没有选任何记录类型。" next="在上面点选要看的类型。" />
      ) : page.isLoading ? (
        <Skeleton rows={8} />
      ) : page.isError ? (
        <ErrorState error={page.error} what="journal 记录" />
      ) : page.data ? (
        <>
          <p className="text-xs text-ink-muted">
            共 {page.data.total} 条所选类型的记录
            {page.data.undecodable > 0 && <span className="text-warn"> · {page.data.undecodable} 条解不开，回放有洞</span>}
          </p>
          <RecordTable page={page.data} emptyNext="这份 journal 里没有所选类型的记录。" />
          <div className="flex gap-2 text-xs">
            <button className="rounded border border-line px-2 py-1 disabled:opacity-40" disabled={!before.length} onClick={() => setBefore(before.slice(0, -1))}>
              较新
            </button>
            <button className="rounded border border-line px-2 py-1 disabled:opacity-40" disabled={!page.data.next_before} onClick={() => setBefore([...before, page.data!.next_before!])}>
              较早
            </button>
          </div>
        </>
      ) : null}
    </div>
  );
}
