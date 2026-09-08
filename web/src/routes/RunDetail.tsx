import { useQuery } from "@tanstack/react-query";
import { useParams } from "react-router-dom";

import { api } from "@/api/client";

import { Failure, Skeleton } from "./Runs";

export function RunDetail() {
  const { id = "" } = useParams();
  const { data, isLoading, error } = useQuery({
    queryKey: ["run", id],
    queryFn: () => api.run(id),
  });

  if (isLoading) return <Skeleton />;
  if (error) return <Failure error={error} />;
  if (!data) return null;

  return (
    <div>
      <h1 className="mb-4 font-mono text-lg text-ink">{data.id}</h1>

      {/* The identity triple is shown together and in full. Truncated to
          eight characters it reads as decoration; at full length it is
          the thing a third party checks a claim against. */}
      <dl className="mb-6 grid gap-x-6 gap-y-2 text-sm sm:grid-cols-[8rem_1fr]">
        <dt className="text-ink-muted">code-commit</dt>
        <dd className="font-mono text-xs break-all">{data.identity.code_commit}</dd>
        <dt className="text-ink-muted">data-sha256</dt>
        <dd className="font-mono text-xs break-all">{data.identity.data_hash}</dd>
        <dt className="text-ink-muted">config-sha256</dt>
        <dd className="font-mono text-xs break-all">{data.identity.config_hash}</dd>
        <dt className="text-ink-muted">档位</dt>
        <dd>{data.identity.label}</dd>
        <dt className="text-ink-muted">已实现盈亏</dt>
        <dd>{data.pnl.toFixed(6)}</dd>
      </dl>

      <h2 className="mb-2 text-sm text-ink-muted">成交 {data.fills.length}</h2>
      <div className="overflow-x-auto rounded border border-line">
        <table className="w-full text-sm">
          <thead className="bg-surface text-left text-xs text-ink-muted">
            <tr>
              <th className="px-3 py-2 font-normal">时间 (ns)</th>
              <th className="px-3 py-2 font-normal">品种</th>
              <th className="px-3 py-2 font-normal">方向</th>
              <th className="px-3 py-2 text-right font-normal">价格 (ticks)</th>
              <th className="px-3 py-2 text-right font-normal">数量 (lots)</th>
              <th className="px-3 py-2 font-normal">tag</th>
            </tr>
          </thead>
          <tbody>
            {data.fills.map((fill, index) => (
              <tr key={index} className="border-t border-line">
                <td className="px-3 py-2 font-mono text-xs">{fill.ts}</td>
                <td className="px-3 py-2 font-mono">{fill.symbol}</td>
                <td
                  className={`px-3 py-2 ${fill.side === "buy" ? "text-good" : "text-bad"}`}
                >
                  {fill.side}
                </td>
                <td className="px-3 py-2 text-right font-mono">{fill.price_ticks}</td>
                <td className="px-3 py-2 text-right font-mono">{fill.qty_lots}</td>
                {/* No tag and an empty tag are different things in the
                    run format, so they look different here too. */}
                <td className="px-3 py-2 text-xs text-ink-muted">
                  {fill.tag === null ? "—" : fill.tag || '""'}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
