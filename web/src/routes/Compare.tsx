import { useQuery } from "@tanstack/react-query";
import { lazy, Suspense, useState } from "react";

import { api, type Verdict } from "@/api/client";

import { Failure } from "./Runs";

// Loaded when a comparison is shown: the chart library is most of the
// bundle, and nobody who never compares two runs should download it.
const MarkoutPanel = lazy(() =>
  import("./Markout").then((m) => ({ default: m.MarkoutPanel })),
);

/**
 * A parity comparison, and the one thing this screen must never do:
 * render "we cannot tell" as if it were "they agree".
 */
export function Compare() {
  const { data: listing } = useQuery({ queryKey: ["runs"], queryFn: api.runs });
  const readable = (listing?.entries ?? []).filter((e) => e.state === "read");

  const [baseline, setBaseline] = useState("");
  const [candidate, setCandidate] = useState("");

  const { data, error, isFetching } = useQuery({
    queryKey: ["compare", baseline, candidate],
    queryFn: () => api.compare(baseline, candidate),
    enabled: baseline !== "" && candidate !== "" && baseline !== candidate,
  });

  return (
    <div>
      <h1 className="mb-4 text-lg text-ink">对比</h1>

      <div className="mb-6 flex flex-wrap gap-3 text-sm">
        <Picker label="基准" value={baseline} onChange={setBaseline} options={readable} />
        <Picker label="待测" value={candidate} onChange={setCandidate} options={readable} />
      </div>

      {error && <Failure error={error} />}
      {isFetching && <p className="text-sm text-ink-muted">比对中…</p>}
      {data && (
        <>
          <VerdictBanner verdict={data.verdict} passes={data.passes} />
          <dl className="mt-4 grid gap-x-6 gap-y-2 text-sm sm:grid-cols-[10rem_1fr]">
            <dt className="text-ink-muted">成交数</dt>
            <dd className="font-mono">
              {data.fill_counts[0]} → {data.fill_counts[1]}
            </dd>
            <dt className="text-ink-muted">盈亏</dt>
            <dd className="font-mono">
              {data.pnl[0].toFixed(6)} → {data.pnl[1].toFixed(6)}
            </dd>
            <dt className="text-ink-muted">相对误差</dt>
            <dd className="font-mono">
              {data.pnl_relative_error === null
                ? "—（基准盈亏为零）"
                : data.pnl_relative_error.toExponential(3)}
            </dd>
            {data.verdict.conclusive && (
              <>
                <dt className="text-ink-muted">差异</dt>
                <dd className="font-mono">{data.differences}</dd>
                <dt className="text-ink-muted">首个分歧</dt>
                <dd className="font-mono">
                  {data.first_divergence === null ? "—" : `#${data.first_divergence}`}
                </dd>
              </>
            )}
          </dl>
          <Suspense fallback={<p className="mt-8 text-sm text-ink-muted">加载图表…</p>}>
            <MarkoutPanel baseline={baseline} candidate={candidate} />
          </Suspense>
        </>
      )}
    </div>
  );
}

function VerdictBanner({ verdict, passes }: { verdict: Verdict; passes: boolean }) {
  if (!verdict.conclusive) {
    // Deliberately not red. A stale baseline is not a regression, and
    // colouring it like one sends someone hunting for a bug that is not
    // there. It is amber, and it says what to do.
    return (
      <div className="rounded border border-warn/40 bg-warn/10 p-4 text-sm">
        <p className="text-ink">基准已失效——无法得出关于引擎的任何结论。</p>
        <ul className="mt-2 list-inside list-disc text-ink-muted">
          {verdict.changed.map((why) => (
            <li key={why}>{why}</li>
          ))}
        </ul>
      </div>
    );
  }
  return (
    <div
      className={`rounded border p-4 text-sm ${
        passes ? "border-good/40 bg-good/10" : "border-bad/40 bg-bad/10"
      }`}
    >
      <p className="text-ink">
        {passes ? "通过" : "存在差异"}
        {verdict.status === "code_changed" && "（代码已变更，数据与配置未变）"}
      </p>
    </div>
  );
}

function Picker({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: { id: string }[];
}) {
  return (
    <label className="flex items-center gap-2">
      <span className="text-ink-muted">{label}</span>
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="rounded border border-line bg-ground px-2 py-1 font-mono text-sm text-ink"
      >
        <option value="">—</option>
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.id}
          </option>
        ))}
      </select>
    </label>
  );
}
