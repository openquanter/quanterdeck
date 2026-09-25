import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";
import { FileWarning, GitCompare } from "lucide-react";

import { api, type RunEntry } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { Badge, Button, Card, Money, PageHeader, Stat, Table, cx } from "@/ui/kit";

/**
 * Backtest runs (docs/UI-V4 §3 研究): the research totals in the header —
 * moved here from the overview — and every run file as a row, including
 * the ones that would not read, each with its reason. Pick two to
 * compare; the first picked is the baseline.
 */
export function Runs() {
  const { data, isLoading, error } = useQuery({ queryKey: ["runs"], queryFn: api.runs });
  const [picked, setPicked] = useState<string[]>([]);
  const navigate = useNavigate();

  const toggle = (id: string) =>
    setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : p.length >= 2 ? p : [...p, id]));

  const header = (
    <PageHeader
      title="回测记录"
      description="运行记录目录里的每一个 run 文件：身份指纹、成交数、已实现盈亏。勾选两份做一致性对比，先勾的是基准。"
      actions={
        <Button
          variant="primary"
          icon={<GitCompare className="h-4 w-4" />}
          disabled={picked.length !== 2}
          title={picked.length !== 2 ? "勾选两份 run" : undefined}
          onClick={() => navigate(`/runs/compare?baseline=${encodeURIComponent(picked[0])}&candidate=${encodeURIComponent(picked[1])}`)}
        >
          对比所选{picked.length ? `（${picked.length}/2）` : ""}
        </Button>
      }
    />
  );

  if (isLoading)
    return (
      <div>
        {header}
        <Skeleton tiles={3} rows={6} />
      </div>
    );
  if (error)
    return (
      <div>
        {header}
        <ErrorState error={error} what="运行记录" />
      </div>
    );
  if (!data) return null;

  const unreadable = data.entries.filter((e) => e.state === "unreadable").length;

  return (
    <div className="space-y-5">
      {header}
      <div className="grid gap-4 sm:grid-cols-3">
        <Stat label="运行记录" value={String(data.entries.length)} sub={`${data.entries.length - unreadable} 个可读`} help="run" />
        <Stat
          label="合计已实现盈亏"
          value={data.total_pnl !== null ? <Money value={data.total_pnl.toFixed(3)} /> : "无法合计"}
          // Said out loud, because a total that quietly excluded a file
          // would be a number the reader cannot check.
          sub={
            data.total_pnl !== null
              ? "所有 run 之和"
              : unreadable > 0
                ? "有文件读不出，合计会悄悄漏掉它"
                : "这些 run 不是同一类，加起来没有意义"
          }
          tone={data.total_pnl === null ? "warn" : undefined}
        />
        <Stat
          label="读不出的文件"
          value={String(unreadable)}
          sub={unreadable > 0 ? "未计入合计；原因见表中对应行" : "全部可读"}
          tone={unreadable > 0 ? "warn" : undefined}
        />
      </div>

      {data.entries.length === 0 ? (
        <Empty title="运行记录目录里还没有 run 文件。" next="回测写出 run 文件后放进 OQ_DECK_RUNS_DIR 指向的目录，这里就会列出来。" />
      ) : (
        <Card bodyClassName="p-0">
          <Table head={["", "run", "code", "data", "config", "档位", <span key="f" className="block text-right">成交</span>, <span key="p" className="block text-right">盈亏</span>]}>
            {data.entries.map((entry: RunEntry) =>
              entry.state === "read" ? (
                <tr key={entry.id} className={cx(picked.includes(entry.id) && "bg-accent/6")}>
                  <td className="w-10">
                    <label className="flex items-center gap-1.5">
                      <input
                        type="checkbox"
                        className="h-4 w-4 accent-[var(--color-accent)]"
                        checked={picked.includes(entry.id)}
                        disabled={!picked.includes(entry.id) && picked.length >= 2}
                        onChange={() => toggle(entry.id)}
                        aria-label={`选择 ${entry.id}`}
                      />
                      {picked.indexOf(entry.id) === 0 && <Badge tone="accent">基准</Badge>}
                      {picked.indexOf(entry.id) === 1 && <Badge tone="accent">待测</Badge>}
                    </label>
                  </td>
                  <td className="font-mono">
                    <Link to={`/runs/${encodeURIComponent(entry.id)}`} className="text-accent hover:underline">
                      {entry.id}
                    </Link>
                  </td>
                  <td className="font-mono text-xs text-ink-muted" title={entry.identity.code_commit}>
                    {entry.identity.code_commit.slice(0, 10)}
                  </td>
                  <td className="font-mono text-xs text-ink-muted" title={entry.identity.data_hash}>
                    {entry.identity.data_hash.slice(0, 10)}
                  </td>
                  <td className="font-mono text-xs text-ink-muted" title={entry.identity.config_hash}>
                    {entry.identity.config_hash.slice(0, 10)}
                  </td>
                  <td className="text-xs">{entry.identity.label}</td>
                  <td className="text-right tabular-nums">{entry.fills}</td>
                  <td className="text-right tabular-nums">{entry.pnl.toFixed(3)}</td>
                </tr>
              ) : (
                <tr key={entry.id} className="bg-warn/5">
                  <td>
                    <FileWarning className="h-4 w-4 text-warn" />
                  </td>
                  <td className="font-mono text-ink-muted">{entry.id}</td>
                  <td className="text-xs text-warn" colSpan={6}>
                    读不出：{entry.error}
                  </td>
                </tr>
              ),
            )}
          </Table>
        </Card>
      )}
    </div>
  );
}
