import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, FilePlus2, FileText } from "lucide-react";

import { ApiError, api, type ReportEntry, type ReportSection } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { tr } from "@/i18n";
import { Badge, Button, Card, Freshness, Money, PageHeader, Table, fmtTime, type Tone } from "@/ui/kit";

/**
 * The reports the deck keeps: one self-contained page per period, written
 * on a schedule and kept 90 days, so what happened is a file rather than
 * something only the live pages knew. A page opens in a new tab and is
 * rendered in this interface's language.
 *
 * The verdict column follows the console's rule: a report whose
 * reconciliation could not be done shows that, never agreement.
 */
export function Reports() {
  const q = useQuery({ queryKey: ["reports"], queryFn: api.reports, refetchInterval: 60_000 });
  const reports = q.data?.reports ?? [];

  return (
    <div className="space-y-5">
      <PageHeader
        title={tr("报告", "Reports")}
        description={tr(
          "每个时段一份自包含的报告：盈亏、权益曲线、对账、事件与主机。按计划生成并保留 90 天；缺的部分会写明原因，不会当成零。",
          "One self-contained report per period: P&L, equity curve, reconciliation, events and host. Written on a schedule and kept 90 days; a missing part says why and is never shown as zero.",
        )}
        meta={<Freshness at={q.dataUpdatedAt} fetching={q.isFetching} staleAfterS={120} onRefresh={() => void q.refetch()} />}
        actions={<GenerateButton />}
      />

      {q.isLoading ? (
        <Skeleton rows={6} />
      ) : q.isError ? (
        <ErrorState error={q.error} what={tr("报告", "reports")} />
      ) : reports.length === 0 ? (
        <Empty
          title={tr("还没有报告。", "No reports yet.")}
          next={tr(
            `deck 每 ${q.data?.every_hours ?? "?"} 小时在时段结束后写一份；也可以点「立即生成」写一份截至现在的。`,
            `The deck writes one every ${q.data?.every_hours ?? "?"} h after each period ends; or press "Generate now" for one up to now.`,
          )}
        />
      ) : (
        <Card title={tr("已保存的报告", "Kept reports")} icon={<FileText className="h-4 w-4" />} bodyClassName="p-0">
          <Table head={[tr("时段", "Period"), tr("对账", "Reconciliation"), tr("净盈亏", "Net P&L"), tr("缺失部分", "Missing parts"), tr("生成", "Generated"), ""]}>
            {reports.map((r) => (
              <Row key={r.id} r={r} />
            ))}
          </Table>
        </Card>
      )}
    </div>
  );
}

function Row({ r }: { r: ReportEntry }) {
  return (
    <tr>
      <td className="whitespace-nowrap text-ink">
        {fmtTime(r.period_from_ms)} – {fmtTime(r.period_to_ms)}
      </td>
      <td>{r.error ? <Badge tone="bad">{tr("文件读不出", "Unreadable file")}</Badge> : <VerdictBadge verdict={r.verdict} />}</td>
      <td className="tabular-nums">{r.net === null ? <span className="text-warn">{tr("未测得", "not measured")}</span> : <Money value={r.net} signed />}</td>
      <td className="text-xs text-ink-muted">{r.error ? <span className="font-mono text-bad">{r.error}</span> : r.unavailable.length === 0 ? "—" : r.unavailable.map(sectionName).join(tr("、", ", "))}</td>
      <td className="whitespace-nowrap text-xs text-ink-muted">
        {r.generated_at_ms !== null ? fmtTime(r.generated_at_ms) : "—"}
        {r.trigger === "manual" && <span className="ml-1">({tr("手动", "on request")})</span>}
      </td>
      <td className="text-right">
        {!r.error && (
          <a href={api.reportUrl(r.id)} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 text-accent hover:underline">
            {tr("打开", "Open")}
            <ExternalLink className="h-3.5 w-3.5" />
          </a>
        )}
      </td>
    </tr>
  );
}

function VerdictBadge({ verdict }: { verdict: ReportEntry["verdict"] }) {
  const [tone, text]: [Tone, string] =
    verdict === "agree"
      ? ["good", tr("一致", "Agree")]
      : verdict === "disagree"
        ? ["bad", tr("不一致", "Disagree")]
        : verdict === "cannot_tell"
          ? ["warn", tr("无法判断", "Cannot tell")]
          : ["neutral", tr("未对账", "Not reconciled")];
  return <Badge tone={tone}>{text}</Badge>;
}

function sectionName(s: ReportSection): string {
  switch (s) {
    case "trader":
      return tr("交易进程", "Trader");
    case "pnl":
      return tr("盈亏", "P&L");
    case "reconciliation":
      return tr("对账", "Reconciliation");
    case "events":
      return tr("事件", "Events");
    case "host":
      return tr("主机", "Host");
  }
}

/** Write one now, for the period ending now. The deck allows one a minute. */
function GenerateButton() {
  const client = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  async function generate() {
    setBusy(true);
    setProblem(null);
    try {
      // Listed, not opened: a tab opened after an await is a popup the
      // browser may block.
      await api.reportGenerate();
      await client.invalidateQueries({ queryKey: ["reports"] });
    } catch (e) {
      if (e instanceof ApiError && e.status === 429) {
        setProblem(e.detail || tr("一分钟内只能生成一次，请稍后再试。", "One report a minute at most; try again shortly."));
      } else {
        setProblem(e instanceof ApiError ? e.detail : String(e));
      }
    } finally {
      setBusy(false);
    }
  }
  return (
    <span className="inline-flex items-center gap-2">
      {problem && (
        <span className="max-w-72 truncate text-xs text-warn" title={problem}>
          {problem}
        </span>
      )}
      <Button variant="primary" icon={<FilePlus2 className="h-4 w-4" />} disabled={busy} onClick={() => void generate()} title={tr("生成一份截至现在的报告（每分钟最多一次）", "Write a report up to now (at most once a minute)")}>
        {busy ? tr("生成中…", "Generating…") : tr("立即生成", "Generate now")}
      </Button>
    </span>
  );
}
