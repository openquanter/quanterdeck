import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";
import { FileWarning, GitCompare } from "lucide-react";

import { api, type RunEntry } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { tr } from "@/i18n";
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
      title={tr("回测记录", "Backtest runs")}
      description={tr(
        "运行记录目录里的每一个 run 文件：身份指纹、成交数、已实现盈亏。勾选两份做一致性对比，先勾的是基准。",
        "Every run file in the runs directory: identity fingerprints, fill count, realized P&L. Check two to compare them for consistency; the first checked is the baseline.",
      )}
      actions={
        <Button
          variant="primary"
          icon={<GitCompare className="h-4 w-4" />}
          disabled={picked.length !== 2}
          title={picked.length !== 2 ? tr("勾选两份 run", "Check two runs") : undefined}
          onClick={() => navigate(`/runs/compare?baseline=${encodeURIComponent(picked[0])}&candidate=${encodeURIComponent(picked[1])}`)}
        >
          {picked.length
            ? tr(`对比所选（${picked.length}/2）`, `Compare selected (${picked.length}/2)`)
            : tr("对比所选", "Compare selected")}
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
        <ErrorState error={error} what={tr("运行记录", "runs")} />
      </div>
    );
  if (!data) return null;

  const unreadable = data.entries.filter((e) => e.state === "unreadable").length;

  return (
    <div className="space-y-5">
      {header}
      <div className="grid gap-4 sm:grid-cols-3">
        <Stat
          label={tr("运行记录", "Runs")}
          value={String(data.entries.length)}
          sub={tr(`${data.entries.length - unreadable} 个可读`, `${data.entries.length - unreadable} readable`)}
          help="run"
        />
        <Stat
          label={tr("合计已实现盈亏", "Total realized P&L")}
          value={data.total_pnl !== null ? <Money value={data.total_pnl.toFixed(3)} /> : tr("无法合计", "Cannot total")}
          // Said out loud, because a total that quietly excluded a file
          // would be a number the reader cannot check.
          sub={
            data.total_pnl !== null
              ? tr("所有 run 之和", "Sum of all runs")
              : unreadable > 0
                ? tr("有文件读不出，合计会悄悄漏掉它", "A file could not be read; a total would silently leave it out")
                : tr("这些 run 不是同一类，加起来没有意义", "These runs are not of one kind; adding them means nothing")
          }
          tone={data.total_pnl === null ? "warn" : undefined}
        />
        <Stat
          label={tr("读不出的文件", "Unreadable files")}
          value={String(unreadable)}
          sub={unreadable > 0 ? tr("未计入合计；原因见表中对应行", "Not in the total; see each row for the reason") : tr("全部可读", "All readable")}
          tone={unreadable > 0 ? "warn" : undefined}
        />
      </div>

      {data.entries.length === 0 ? (
        <Empty
          title={tr("运行记录目录里还没有 run 文件。", "No run files in the runs directory yet.")}
          next={tr(
            "回测写出 run 文件后放进 OQ_DECK_RUNS_DIR 指向的目录，这里就会列出来。",
            "Put the run files a backtest writes into the directory OQ_DECK_RUNS_DIR points to, and they are listed here.",
          )}
        />
      ) : (
        <Card bodyClassName="p-0">
          <Table
            head={[
              "",
              "run",
              "code",
              "data",
              "config",
              tr("档位", "Label"),
              <span key="f" className="block text-right">
                {tr("成交", "Fills")}
              </span>,
              <span key="p" className="block text-right">
                {tr("盈亏", "P&L")}
              </span>,
            ]}
          >
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
                        aria-label={tr(`选择 ${entry.id}`, `Select ${entry.id}`)}
                      />
                      {picked.indexOf(entry.id) === 0 && <Badge tone="accent">{tr("基准", "Baseline")}</Badge>}
                      {picked.indexOf(entry.id) === 1 && <Badge tone="accent">{tr("待测", "Candidate")}</Badge>}
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
                    {tr("读不出：", "Unreadable: ")}
                    {entry.error}
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
