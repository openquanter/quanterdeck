import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link2, Link2Off, ScrollText } from "lucide-react";

import { api, type AuditEntry } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { Ago, Badge, Card, Freshness, PageHeader, Segmented, Table, cx, fmtTime, type Tone } from "@/ui/kit";

const LINES = [100, 300, 1000] as const;

/**
 * The agent's audit trail, and whether its hash chain still holds. The
 * chain's verdict leads: a broken chain means the entries below can no
 * longer be trusted to be the whole story, whatever they say.
 */
export function Audit() {
  const [lines, setLines] = useState<number>(300);
  const audit = useQuery({ queryKey: ["ops", "audit", lines], queryFn: () => api.audit(lines), refetchInterval: 15_000 });
  const a = audit.data;

  return (
    <div className="space-y-5">
      <PageHeader
        title="审计日志"
        description="每一次经主机代理的操作：谁、什么时候、做了什么、为什么、结果如何。每一条也同步发到了告警频道。"
        meta={<Freshness at={audit.dataUpdatedAt} fetching={audit.isFetching} staleAfterS={60} onRefresh={() => void audit.refetch()} />}
      />

      {audit.isLoading ? (
        <Skeleton tiles={1} rows={8} />
      ) : audit.isError ? (
        <ErrorState error={audit.error} what="审计记录" />
      ) : a ? (
        <>
          <div
            className={cx(
              "flex items-start gap-3 rounded-[var(--radius-card)] border px-5 py-4",
              a.chain.intact ? "border-good/30 bg-good/6" : "border-bad/40 bg-bad/8",
            )}
          >
            {a.chain.intact ? <Link2 className="mt-0.5 h-6 w-6 shrink-0 text-good" /> : <Link2Off className="mt-0.5 h-6 w-6 shrink-0 text-bad" />}
            <div className="min-w-0">
              <div className="text-lg font-semibold text-ink">{a.chain.intact ? "哈希链完整" : "哈希链断裂"}</div>
              <p className="mt-0.5 text-sm text-ink-muted">
                {a.chain.intact ? "每一条都与前一条的哈希相连，记录没有被改动或删除。" : "记录可能被改动或删除过，下面的条目不能当作完整的历史。"}
              </p>
              {!a.chain.intact && a.chain.problem && <p className="mt-1.5 break-words font-mono text-xs text-bad">{a.chain.problem}</p>}
            </div>
          </div>

          <Card
            title="操作记录"
            icon={<ScrollText className="h-4 w-4" />}
            extra={
              <>
                <span>最后</span>
                <Segmented value={lines} options={LINES.map((n) => ({ value: n, label: `${n} 条` }))} onChange={setLines} />
              </>
            }
            bodyClassName="p-0"
          >
            {a.entries.length === 0 ? (
              <div className="p-4">
                <Empty title="还没有操作记录。" next="停机、启停服务、部署等操作执行时会在这里留下一条。" />
              </div>
            ) : (
              <Table head={["#", "时间", "谁", "操作", "原因", "结果"]}>
                {[...a.entries].reverse().map((e) => (
                  <tr key={e.seq} className="align-top">
                    <td className="font-mono text-xs text-ink-faint" title={e.hash}>
                      {e.seq}
                    </td>
                    <td className="whitespace-nowrap text-xs text-ink-muted">
                      <div className="text-ink">
                        <Ago ms={e.at_ms} />
                      </div>
                      <div className="text-ink-faint">{fmtTime(e.at_ms)}</div>
                    </td>
                    <td className="font-mono text-xs text-ink">{e.actor}</td>
                    <td className="text-ink">{e.op}</td>
                    <td className="text-ink-muted">{e.reason || <span className="text-ink-faint">—</span>}</td>
                    <td>
                      <Result e={e} />
                    </td>
                  </tr>
                ))}
              </Table>
            )}
          </Card>
        </>
      ) : null}
    </div>
  );
}

/** The result as a verdict, with the agent's own words for a refusal. */
function Result({ e }: { e: AuditEntry }) {
  const r = e.result;
  const m = r.match(/^(refused|failed|error)[:\s]*([\s\S]*)$/);
  let tone: Tone = "neutral";
  let label = r;
  let why: string | undefined;
  if (r === "done") {
    tone = "good";
    label = "完成";
  } else if (r === "requested") {
    label = "已请求";
  } else if (m) {
    tone = "bad";
    label = m[1] === "refused" ? "被拒绝" : "失败";
    why = m[2] || undefined;
  }
  return (
    <div className="min-w-0">
      <Badge tone={tone} dot={tone !== "neutral"}>
        {label}
      </Badge>
      {why && <div className="mt-1 break-words text-xs text-ink-muted">{why}</div>}
    </div>
  );
}
