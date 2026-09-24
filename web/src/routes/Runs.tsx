import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { api, type RunEntry } from "@/api/client";

export function Runs() {
  const { data, isLoading, error } = useQuery({ queryKey: ["runs"], queryFn: api.runs });

  if (isLoading) return <Skeleton />;
  if (error) return <Failure error={error} />;
  if (!data) return null;

  const unreadable = data.entries.filter((e) => e.state === "unreadable").length;

  return (
    <div>
      <h1 className="mb-1 text-lg text-ink">运行记录</h1>
      <p className="mb-4 text-sm text-ink-muted">
        {data.entries.length} 个文件 · 合计已实现盈亏{" "}
        <span className="text-ink">
          {data.total_pnl !== null ? data.total_pnl.toFixed(3) : "—（无法合计）"}
        </span>
        {unreadable > 0 && (
          // Said out loud, because a total that quietly excluded a file
          // would be a number the reader cannot check.
          <> · {unreadable} 个未能读取，未计入合计</>
        )}
      </p>

      <div className="overflow-x-auto rounded border border-line">
        <table className="w-full text-sm">
          <thead className="bg-surface text-left text-xs text-ink-muted">
            <tr>
              <th className="px-3 py-2 font-normal">run</th>
              <th className="px-3 py-2 font-normal">code</th>
              <th className="px-3 py-2 font-normal">data</th>
              <th className="px-3 py-2 font-normal">config</th>
              <th className="px-3 py-2 font-normal">档位</th>
              <th className="px-3 py-2 text-right font-normal">成交</th>
              <th className="px-3 py-2 text-right font-normal">盈亏</th>
            </tr>
          </thead>
          <tbody>
            {data.entries.map((entry: RunEntry) =>
              entry.state === "read" ? (
                <tr key={entry.id} className="border-t border-line">
                  <td className="px-3 py-2 font-mono">
                    <Link to={`/runs/${entry.id}`} className="text-accent hover:underline">
                      {entry.id}
                    </Link>
                  </td>
                  <td className="px-3 py-2 font-mono text-xs text-ink-muted">
                    {entry.identity.code_commit.slice(0, 10)}
                  </td>
                  <td className="px-3 py-2 font-mono text-xs text-ink-muted">
                    {entry.identity.data_hash.slice(0, 10)}
                  </td>
                  <td className="px-3 py-2 font-mono text-xs text-ink-muted">
                    {entry.identity.config_hash.slice(0, 10)}
                  </td>
                  <td className="px-3 py-2 text-xs">{entry.identity.label}</td>
                  <td className="px-3 py-2 text-right">{entry.fills}</td>
                  <td className="px-3 py-2 text-right">{entry.pnl.toFixed(3)}</td>
                </tr>
              ) : (
                <tr key={entry.id} className="border-t border-line bg-bad/5">
                  <td className="px-3 py-2 font-mono text-ink-muted">{entry.id}</td>
                  <td className="px-3 py-2 text-xs text-bad" colSpan={6}>
                    {entry.error}
                  </td>
                </tr>
              ),
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

export function Skeleton() {
  return (
    <div className="space-y-2">
      {Array.from({ length: 6 }).map((_, index) => (
        <div key={index} className="h-8 animate-pulse rounded bg-surface" />
      ))}
    </div>
  );
}

export function Failure({ error }: { error: unknown }) {
  return (
    <div className="rounded border border-bad/40 bg-bad/10 p-4 text-sm">
      <p className="text-ink">读取失败。</p>
      <p className="mt-2 font-mono text-xs text-ink-muted">
        {error instanceof Error ? error.message : String(error)}
      </p>
    </div>
  );
}
