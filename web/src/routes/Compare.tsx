import { lazy, Suspense, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router-dom";
import { ArrowLeft, ArrowRight, CheckCircle2, Code2, RefreshCcw, TriangleAlert, XCircle } from "lucide-react";

import { api, type Comparison, type Verdict } from "@/api/client";
import { ErrorState, Skeleton } from "@/components/States";
import { tr } from "@/i18n";
import { Badge, Card, KV, PageHeader, cx } from "@/ui/kit";

// Loaded when a comparison is shown: the chart library is most of the
// bundle, and nobody who never compares two runs should download it.
const MarkoutPanel = lazy(() => import("./Markout").then((m) => ({ default: m.MarkoutPanel })));

const SELECT = "h-8 rounded-md border border-line-strong bg-surface-raised px-2.5 font-mono text-xs text-ink";

/**
 * A parity comparison of two runs (docs/UI-V4 §3 研究), and the one thing
 * this screen must never do: render "we cannot tell" as if it were "they
 * agree". The pair and the tolerance live in the URL, so the runs list
 * can link straight to a comparison and a comparison can be shared.
 */
export function Compare() {
  const { data: listing } = useQuery({ queryKey: ["runs"], queryFn: api.runs });
  const readable = (listing?.entries ?? []).filter((e) => e.state === "read");

  const [params, setParams] = useSearchParams();
  const baseline = params.get("baseline") ?? "";
  const candidate = params.get("candidate") ?? "";
  const tolText = params.get("tolerance") ?? "0";
  const tolerance = Number.isFinite(Number(tolText)) && Number(tolText) >= 0 ? Number(tolText) : 0;
  const set = (k: string, v: string) => {
    const next = new URLSearchParams(params);
    if (v === "") next.delete(k);
    else next.set(k, v);
    setParams(next, { replace: true });
  };

  const same = baseline !== "" && baseline === candidate;
  const { data, error, isFetching } = useQuery({
    queryKey: ["compare", baseline, candidate, tolerance],
    queryFn: () => api.compare(baseline, candidate, tolerance),
    enabled: baseline !== "" && candidate !== "" && !same,
  });

  return (
    <div className="space-y-5">
      <PageHeader
        title={tr("对比", "Compare")}
        description={tr(
          "同样的数据和配置下，两次运行的成交是否一致；以及成交之后价格往哪走。",
          "Whether two runs on the same data and config produce the same fills, and where the price went after each fill.",
        )}
        actions={
          <Link to="/runs" className="inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-sm text-ink-muted hover:bg-surface-hover hover:text-ink">
            <ArrowLeft className="h-4 w-4" />
            {tr("回测记录", "Backtest runs")}
          </Link>
        }
      />

      <Card bodyClassName="flex flex-wrap items-end gap-4 p-4">
        <Picker label={tr("基准", "Baseline")} value={baseline} onChange={(v) => set("baseline", v)} options={readable} />
        <ArrowRight className="mb-2 h-4 w-4 text-ink-faint" />
        <Picker label={tr("待测", "Candidate")} value={candidate} onChange={(v) => set("candidate", v)} options={readable} />
        <label className="text-xs text-ink-muted">
          <span className="mb-1.5 block">{tr("盈亏相对误差容忍", "P&L relative error tolerance")}</span>
          <input
            className={cx(SELECT, "w-28 tabular-nums")}
            key={tolText}
            inputMode="decimal"
            defaultValue={tolText}
            onBlur={(e) => {
              const n = Number(e.target.value);
              if (e.target.value.trim() !== "" && Number.isFinite(n) && n >= 0) set("tolerance", n === 0 ? "" : String(n));
              else e.target.value = String(tolerance);
            }}
          />
        </label>
        {isFetching && <span className="mb-2 text-xs text-ink-faint">{tr("比对中…", "Comparing…")}</span>}
      </Card>

      {same && <p className="text-sm text-ink-muted">{tr("基准和待测是同一份 run，选两份不同的。", "Baseline and candidate are the same run; pick two different ones.")}</p>}
      {!baseline || !candidate ? (
        <p className="text-sm text-ink-muted">
          {tr(
            "选一份基准和一份待测 run；也可以在「回测记录」里勾选两份后点「对比所选」。",
            "Pick a baseline run and a candidate run, or check two in Backtest runs and click Compare selected.",
          )}
        </p>
      ) : null}
      {error ? <ErrorState error={error} what={tr("对比结果", "comparison")} /> : null}
      {isFetching && !data && <Skeleton tiles={1} rows={4} />}
      {data && (
        <>
          <VerdictBanner c={data} />
          <Numbers c={data} />
          <Suspense fallback={<Skeleton rows={4} />}>
            <MarkoutPanel baseline={baseline} candidate={candidate} />
          </Suspense>
        </>
      )}
    </div>
  );
}

/**
 * Three outcomes, three looks. Comparable: agree (green) or differ (red).
 * Code changed: the same verdict, marked as a claim about the new code.
 * Invalidated: amber and never green — a stale baseline says nothing
 * about the engine, and it says what to do instead.
 */
function VerdictBanner({ c }: { c: Comparison }) {
  const verdict: Verdict = c.verdict;
  if (!verdict.conclusive) {
    // Deliberately not red. A stale baseline is not a regression, and
    // colouring it like one sends someone hunting for a bug that is not
    // there. It is amber, and it says what to do.
    return (
      <Banner
        tone="warn"
        icon={<TriangleAlert className="h-6 w-6 text-warn" />}
        title={tr("基准已失效——无法得出任何结论", "Baseline invalidated: no conclusion can be drawn")}
        badge={<Badge tone="warn">{tr("不是「一致」，也不是「不一致」", "Neither \"match\" nor \"differ\"")}</Badge>}
      >
        <p className="text-ink-muted">
          {tr(
            "两份 run 的数据或配置不同，比较它们的成交说明不了引擎有没有变。",
            "The two runs differ in data or config, so comparing their fills says nothing about whether the engine changed.",
          )}
        </p>
        {verdict.changed.length > 0 && (
          <ul className="mt-2 space-y-1">
            {verdict.changed.map((why) => (
              <li key={why} className="flex gap-2 text-ink">
                <span className="text-warn">•</span>
                {why}
              </li>
            ))}
          </ul>
        )}
        <p className="mt-3 flex items-start gap-2 rounded-md border border-warn/30 bg-ground/40 px-3 py-2 text-ink">
          <RefreshCcw className="mt-0.5 h-4 w-4 shrink-0 text-warn" />
          {tr(
            "需要重建基准（rebase）：用与待测相同的数据和配置重新跑一份基准，再拿来对比。在那之前，这里的任何数字都不能当作结论。",
            "Rebase needed: rerun the baseline on the same data and config as the candidate, then compare again. Until then, no number here is a conclusion.",
          )}
        </p>
      </Banner>
    );
  }
  const codeChanged = verdict.status === "code_changed";
  return (
    <Banner
      tone={c.passes ? "good" : "bad"}
      icon={c.passes ? <CheckCircle2 className="h-6 w-6 text-good" /> : <XCircle className="h-6 w-6 text-bad" />}
      title={
        c.passes
          ? tr("一致：通过", "Match: pass")
          : tr(`存在差异：${c.differences} 处`, `Differences: ${c.differences}`)
      }
      badge={
        codeChanged ? (
          <Badge tone="accent">
            <Code2 className="h-3 w-3" />
            {tr("代码已变更，数据与配置未变", "Code changed; data and config unchanged")}
          </Badge>
        ) : (
          <Badge>{tr("代码、数据、配置都相同", "Same code, data and config")}</Badge>
        )
      }
    >
      <p className="text-ink-muted">
        {codeChanged
          ? c.passes
            ? tr(
                "新代码在同样的数据和配置下跑出了同样的成交：这次改动没有改变行为。",
                "The new code produced the same fills on the same data and config: this change did not alter behavior.",
              )
            : tr(
                "新代码在同样的数据和配置下跑出了不同的成交：这次改动改变了行为，确认是否有意为之。",
                "The new code produced different fills on the same data and config: this change altered behavior. Confirm it was intended.",
              )
          : c.passes
            ? tr("同样的代码、数据、配置跑出了同样的成交。", "The same code, data and config produced the same fills.")
            : tr(
                "同样的代码、数据、配置却跑出了不同的成交：这是行为回归，需要查。",
                "The same code, data and config produced different fills: this is a behavior regression and needs investigating.",
              )}
      </p>
      {codeChanged && verdict.changed.length > 0 && (
        <ul className="mt-2 space-y-0.5 text-xs text-ink-muted">
          {verdict.changed.map((why) => (
            <li key={why}>· {why}</li>
          ))}
        </ul>
      )}
    </Banner>
  );
}

function Banner({ tone, icon, title, badge, children }: { tone: "good" | "bad" | "warn"; icon: ReactNode; title: string; badge?: ReactNode; children: ReactNode }) {
  return (
    <div
      className={cx(
        "rounded-[var(--radius-card)] border px-5 py-4 text-sm",
        tone === "good" ? "border-good/30 bg-good/6" : tone === "bad" ? "border-bad/40 bg-bad/8" : "border-warn/40 bg-warn/8",
      )}
    >
      <div className="flex flex-wrap items-center gap-3">
        {icon}
        <div className="text-lg font-semibold text-ink">{title}</div>
        {badge}
      </div>
      <div className="mt-2">{children}</div>
    </div>
  );
}

function Numbers({ c }: { c: Comparison }) {
  const items: [ReactNode, ReactNode][] = [
    [tr("成交数", "Fills"), <span key="f" className="font-mono">{c.fill_counts[0]} → {c.fill_counts[1]}</span>],
    [tr("盈亏", "P&L"), <span key="p" className="font-mono">{c.pnl[0].toFixed(6)} → {c.pnl[1].toFixed(6)}</span>],
    [
      tr("相对误差", "Relative error"),
      <span key="e" className="font-mono">
        {c.pnl_relative_error === null ? tr("—（基准盈亏为零）", "— (baseline P&L is zero)") : c.pnl_relative_error.toExponential(3)}
      </span>,
    ],
  ];
  // Differences and the first divergence mean nothing when the baseline is
  // invalidated, so they are not shown at all rather than shown as 0.
  if (c.verdict.conclusive) {
    items.push(
      [tr("差异", "Differences"), <span key="d" className="font-mono">{c.differences}</span>],
      [
        tr("一致的前缀", "Matching prefix"),
        <span key="m" className="font-mono">
          {tr(`${c.matched_prefix} 笔`, `${c.matched_prefix} fill${c.matched_prefix === 1 ? "" : "s"}`)}
        </span>,
      ],
      [tr("首个分歧", "First divergence"), <span key="v" className="font-mono">{c.first_divergence === null ? "—" : `#${c.first_divergence}`}</span>],
    );
  }
  return (
    <Card
      title={
        <span className="font-mono text-xs">
          {c.baseline} <span className="text-ink-faint">→</span> {c.candidate}
        </span>
      }
    >
      <KV cols={3} items={items} />
    </Card>
  );
}

function Picker({ label, value, onChange, options }: { label: string; value: string; onChange: (value: string) => void; options: { id: string }[] }) {
  return (
    <label className="text-xs text-ink-muted">
      <span className="mb-1.5 block">{label}</span>
      <select value={value} onChange={(event) => onChange(event.target.value)} className={cx(SELECT, "min-w-56")}>
        <option value="">—</option>
        {/* A run named in the URL but no longer listed stays selectable, so
            the link still says what it compared. */}
        {value && !options.some((o) => o.id === value) && <option value={value}>{value}</option>}
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.id}
          </option>
        ))}
      </select>
    </label>
  );
}
