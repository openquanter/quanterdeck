import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";
import { ArrowLeft, Fingerprint } from "lucide-react";

import { api } from "@/api/client";
import { Copy, ErrorState, Skeleton } from "@/components/States";
import { Card, PageHeader, Stat, Table } from "@/ui/kit";

/**
 * One run file (docs/UI-V4 §3 研究): its identity in full, the numbers
 * it adds up to, and every fill it recorded.
 */
export function RunDetail() {
  const { id = "" } = useParams();
  const { data, isLoading, error } = useQuery({
    queryKey: ["run", id],
    queryFn: () => api.run(id),
  });

  const back = (
    <Link to="/runs" className="inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-sm text-ink-muted hover:bg-surface-hover hover:text-ink">
      <ArrowLeft className="h-4 w-4" />
      回测记录
    </Link>
  );

  if (isLoading)
    return (
      <div>
        <PageHeader title={<span className="font-mono">{id}</span>} actions={back} />
        <Skeleton tiles={3} rows={8} />
      </div>
    );
  if (error)
    return (
      <div>
        <PageHeader title={<span className="font-mono">{id}</span>} actions={back} />
        <ErrorState error={error} what={`run ${id}`} />
      </div>
    );
  if (!data) return null;

  const symbols = [...new Set(data.fills.map((f) => f.symbol))];

  return (
    <div className="space-y-5">
      <PageHeader title={<span className="font-mono">{data.id}</span>} description={<span className="font-mono text-xs">{data.path}</span>} actions={back} />

      <div className="grid gap-4 sm:grid-cols-3">
        <Stat label="已实现盈亏" value={data.pnl.toFixed(6)} />
        <Stat label="成交" value={String(data.fills.length)} sub={symbols.length ? symbols.join(" · ") : undefined} />
        <Stat label="档位" value={data.identity.label || "—"} />
      </div>

      {/* The identity triple is shown together and in full. Truncated to
          eight characters it reads as decoration; at full length it is
          the thing a third party checks a claim against. */}
      <Card title="身份" icon={<Fingerprint className="h-4 w-4" />} extra="代码、数据、配置的指纹：同样三者跑出的结果应当一致">
        <dl className="grid gap-x-6 gap-y-2.5 text-sm sm:grid-cols-[8rem_1fr]">
          {(
            [
              ["code-commit", data.identity.code_commit],
              ["data-sha256", data.identity.data_hash],
              ["config-sha256", data.identity.config_hash],
            ] as const
          ).map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="text-ink-muted">{k}</dt>
              <dd className="flex items-baseline break-all font-mono text-xs text-ink">
                <span className="min-w-0">{v}</span>
                <Copy text={v} />
              </dd>
            </div>
          ))}
        </dl>
      </Card>

      <Card title={`成交（${data.fills.length}）`} bodyClassName="p-0">
        {data.fills.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-ink-faint">这次运行没有成交。</p>
        ) : (
          <div className="max-h-[40rem] overflow-auto">
            <Table
              dense
              head={[
                "时间 (ns)",
                "品种",
                "方向",
                <span key="p" className="block text-right">
                  价格 (ticks)
                </span>,
                <span key="q" className="block text-right">
                  数量 (lots)
                </span>,
                "tag",
              ]}
            >
              {data.fills.map((fill, index) => (
                <tr key={index}>
                  <td className="font-mono text-xs text-ink-muted">{fill.ts}</td>
                  <td className="font-mono">{fill.symbol}</td>
                  {/* Neutral: green and red mean a conclusion held or
                      failed (UI-BRIEF §8), and a sell is neither. */}
                  <td className="text-ink">{fill.side}</td>
                  <td className="text-right font-mono tabular-nums">{fill.price_ticks}</td>
                  <td className="text-right font-mono tabular-nums">{fill.qty_lots}</td>
                  {/* No tag and an empty tag are different things in the
                      run format, so they look different here too. */}
                  <td className="text-xs text-ink-muted">{fill.tag === null ? "—" : fill.tag || '""'}</td>
                </tr>
              ))}
            </Table>
          </div>
        )}
      </Card>
    </div>
  );
}
