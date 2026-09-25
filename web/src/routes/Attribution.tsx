import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, CheckCircle2, CircleDashed, CircleHelp, Equal, FileStack, Layers } from "lucide-react";

import { api, type AttributionComponent, type AttributionReport, type RunEntry } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { Badge, Card, Help, PageHeader, Segmented, Stat, cx } from "@/ui/kit";

/**
 * Live minus model, decomposed (UI-BRIEF §4.5).
 *
 * Three states per cause that must never look alike: measured and
 * non-zero, measured and zero, and not measured. And one rule above all:
 * a residual that could not be computed is **unknown**, never 0 — a zero
 * residual from an incomplete decomposition claims everything was
 * explained, which is the one lie this product must not tell.
 */
export function Attribution({ embedded }: { embedded?: boolean } = {}) {
  const caps = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const [source, setSource] = useState<"shadow" | "runs">("shadow");
  useEffect(() => {
    if (caps.data && !caps.data.ops.available && caps.data.runs.available) setSource("runs");
  }, [caps.data]);

  return (
    <div className="space-y-5">
      {!embedded && <PageHeader title="盈亏归因" description="实盘盈亏与模型盈亏的差额，拆成五个成因和残差。" />}
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-sm text-ink-muted">来源</span>
        <Segmented
          value={source}
          onChange={setSource}
          options={[
            ...(caps.data?.ops.available ? [{ value: "shadow" as const, label: "交易进程实时（shadow）" }] : []),
            ...(caps.data?.runs.available ? [{ value: "runs" as const, label: "两份 run 文件" }] : []),
          ]}
        />
        <span className="text-xs text-ink-faint">
          {source === "shadow"
            ? "来源最强：交易进程自己的 shadow 回测和实盘成交，带每笔成交时的市价，滑点和延迟可以分开算。"
            : "来源较弱：run 文件没有成交时的市价，滑点和延迟会是「不可得」。费用和资金费需要你提供交易所账单上的数。"}
        </span>
      </div>
      {source === "shadow" ? <ShadowSource /> : <RunsSource />}
    </div>
  );
}

function ShadowSource() {
  const q = useQuery({ queryKey: ["ops", "attribution"], queryFn: api.opsAttribution, refetchInterval: 60_000 });
  if (q.isLoading) return <Skeleton tiles={4} rows={5} />;
  if (q.isError) return <ErrorState error={q.error} what="交易进程的归因" />;
  if (!q.data) return null;
  return <Report r={q.data} />;
}

function RunsSource() {
  const runs = useQuery({ queryKey: ["runs"], queryFn: api.runs });
  const readable = (runs.data?.entries ?? []).filter((e): e is Extract<RunEntry, { state: "read" }> => e.state === "read");
  const [live, setLive] = useState("");
  const [model, setModel] = useState("");
  const [priceScale, setPriceScale] = useState("");
  const [qtyScale, setQtyScale] = useState("");
  const [fees, setFees] = useState({ venue: "", model: "" });
  const [funding, setFunding] = useState({ venue: "", model: "" });

  // The live runs a process wrote pair with the model run beside them.
  useEffect(() => {
    if (!live && readable.length) {
      const l = readable.find((r) => r.id.endsWith(".live")) ?? readable[0];
      setLive(l.id);
      const m = readable.find((r) => r.id === l.id.replace(/\.live$/, ".model"));
      if (m) setModel(m.id);
    }
  }, [readable, live]);

  const ready = live && model && priceScale !== "" && qtyScale !== "";
  const q = useQuery({
    queryKey: ["attribution", live, model, priceScale, qtyScale, fees, funding],
    queryFn: () =>
      api.attribution({
        live,
        model,
        price_scale: Number(priceScale),
        qty_scale: Number(qtyScale),
        venue_fees: fees.venue,
        model_fees: fees.model,
        venue_funding: funding.venue,
        model_funding: funding.model,
      }),
    enabled: Boolean(ready),
  });

  if (runs.isLoading) return <Skeleton rows={4} />;
  if (runs.isError) return <ErrorState error={runs.error} what="运行记录" />;
  if (readable.length < 2) {
    return (
      <Empty
        title="需要至少两份可读的 run 文件：一份实盘、一份模型。"
        next="交易进程每 15 分钟会在 journal 旁写出 .live.run 和 .model.run；刚启动的进程要等第一次写出。"
      />
    );
  }
  return (
    <div className="space-y-5">
      <Card title="选择 run 文件" icon={<FileStack className="h-4 w-4" />}>
        <div className="grid gap-4 text-xs text-ink-muted sm:grid-cols-2">
          <Field label="实盘 run">
            <select className={cx(INPUT, "w-full")} value={live} onChange={(e) => setLive(e.target.value)}>
              {readable.map((r) => (
                <option key={r.id}>{r.id}</option>
              ))}
            </select>
          </Field>
          <Field label="模型 run">
            <select className={cx(INPUT, "w-full")} value={model} onChange={(e) => setModel(e.target.value)}>
              <option value="">选择…</option>
              {readable.map((r) => (
                <option key={r.id}>{r.id}</option>
              ))}
            </select>
          </Field>
          <div className="grid grid-cols-2 gap-4">
            <Field label="价格精度（小数位，必填）">
              <input className={cx(INPUT, "w-24")} inputMode="numeric" value={priceScale} onChange={(e) => setPriceScale(e.target.value.replace(/\D/g, ""))} />
            </Field>
            <Field label="数量精度（小数位，必填）">
              <input className={cx(INPUT, "w-24")} inputMode="numeric" value={qtyScale} onChange={(e) => setQtyScale(e.target.value.replace(/\D/g, ""))} />
            </Field>
          </div>
          <div />
          <Pair label="手续费（交易所账单 / 模型）" value={fees} onChange={setFees} />
          <Pair label="资金费（交易所账单 / 模型）" value={funding} onChange={setFunding} />
        </div>
        <p className="mt-4 border-t border-line pt-3 text-xs text-ink-muted">
          精度没有默认值：填错会让每个成因差一个数量级。手续费和资金费要成对填；只填一半等于没填。
        </p>
      </Card>
      {!ready ? (
        <p className="text-sm text-ink-faint">选好两份 run 并填上价格精度和数量精度后，这里显示归因。</p>
      ) : q.isLoading ? (
        <Skeleton tiles={4} rows={5} />
      ) : q.isError ? (
        <ErrorState error={q.error} what="归因" />
      ) : q.data ? (
        <Report r={q.data} />
      ) : null}
    </div>
  );
}

const INPUT = "h-8 rounded-md border border-line-strong bg-ground px-2.5 font-mono text-xs text-ink outline-none focus:border-accent";

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1.5 block">{label}</span>
      {children}
    </label>
  );
}

function Pair({
  label,
  value,
  onChange,
}: {
  label: string;
  value: { venue: string; model: string };
  onChange: (v: { venue: string; model: string }) => void;
}) {
  const num = (s: string) => s.replace(/[^\d.-]/g, "");
  return (
    <Field label={label}>
      <div className="flex gap-2">
        <input className={cx(INPUT, "w-28")} placeholder="交易所" value={value.venue} onChange={(e) => onChange({ ...value, venue: num(e.target.value) })} />
        <input className={cx(INPUT, "w-28")} placeholder="模型" value={value.model} onChange={(e) => onChange({ ...value, model: num(e.target.value) })} />
      </div>
    </Field>
  );
}

const NAMES: Record<string, string> = {
  slippage: "滑点",
  "queue position": "排队位置",
  latency: "延迟",
  "funding vs model": "资金费",
  "fee tier": "手续费档位",
};

function fmt(n: number) {
  return (n >= 0 ? "+" : "") + n.toFixed(4);
}

type CauseState = "measured" | "zero" | "unavailable";

function stateOf(c: AttributionComponent): CauseState {
  return c.amount === null ? "unavailable" : c.amount === 0 ? "zero" : "measured";
}

function Report({ r }: { r: AttributionReport }) {
  const missing = r.components.filter((c) => c.amount === null);
  return (
    <div className="space-y-5">
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <Stat label="实盘盈亏" value={<Signed n={r.live_pnl} />} sub={r.live_run ?? "交易进程的实盘成交"} />
        <Stat label="模型盈亏" value={<Signed n={r.model_pnl} />} sub={r.model_run ?? "同样行情下的 shadow 回测"} help="shadow" />
        <Stat label="差额（实盘 − 模型）" value={<span className="tabular-nums">{fmt(r.gap)}</span>} sub="要解释的部分" />
        <ResidualStat r={r} missing={missing.length} />
      </div>

      {r.missing_inputs.length > 0 && (
        <div className="flex gap-3 rounded-[var(--radius-card)] border border-warn/40 bg-warn/8 px-5 py-4">
          <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-warn" />
          <div className="min-w-0">
            <div className="text-sm font-semibold text-ink">要看到完整答案，还缺：</div>
            <ul className="mt-1.5 space-y-1 text-sm text-ink">
              {r.missing_inputs.map((m) => (
                <li key={m} className="flex gap-2">
                  <span className="text-warn">•</span>
                  {m}
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}

      <div className="grid gap-5 xl:grid-cols-5">
        <Card title="五个成因" icon={<Layers className="h-4 w-4" />} className="xl:col-span-2" bodyClassName="space-y-2 p-3">
          {r.components.map((c) => (
            <Cause key={c.name} c={c} />
          ))}
        </Card>
        <Card title="从模型到实盘" className="xl:col-span-3" extra={<Legend />}>
          <Waterfall r={r} />
        </Card>
      </div>

      <p className="text-xs text-ink-faint">
        来源 {r.method === "shadow" ? "交易进程 shadow（实时）" : "run 文件"} · 匹配成交 {r.matched_fills} · 单边成交 {r.unmatched_fills}
        {r.live_run && ` · ${r.live_run} 对 ${r.model_run}`}
      </p>
    </div>
  );
}

function Signed({ n }: { n: number }) {
  return <span className={cx("tabular-nums", n < 0 ? "text-bad" : "text-ink")}>{fmt(n)}</span>;
}

/**
 * The residual as its own tile, so "unknown" can look nothing like a
 * number: amber, dashed, with the count of causes that were not measured.
 */
function ResidualStat({ r, missing }: { r: AttributionReport; missing: number }) {
  if (r.residual === null) {
    return (
      <div className="rounded-[var(--radius-card)] border border-dashed border-warn/60 bg-warn/8 px-4 py-3.5">
        <div className="flex items-center gap-1.5 text-xs text-warn">
          <CircleHelp className="h-3.5 w-3.5" />
          <span>残差</span>
          <Help term="residual" />
        </div>
        <div className="mt-1.5 text-2xl font-semibold tracking-tight text-warn">未知</div>
        <div className="mt-1 text-xs text-ink-muted">有 {missing} 个成因没测到，残差无法计算——不是 0</div>
      </div>
    );
  }
  return (
    <Stat
      label="残差"
      help="residual"
      value={<span className="tabular-nums">{fmt(r.residual)}</span>}
      sub={r.residual_share !== null ? `占实盘盈亏 ${(r.residual_share * 100).toFixed(1)}%` : "五个成因都解释不了的部分"}
    />
  );
}

/** One cause, in one of three looks that cannot be mistaken for each other. */
function Cause({ c }: { c: AttributionComponent }) {
  const state = stateOf(c);
  const nature = c.observed ? "两个观测量之差" : "需要判断价值";
  return (
    <div
      className={cx(
        "rounded-md border px-3 py-2.5",
        state === "measured" && "border-line-strong bg-surface-raised",
        state === "zero" && "border-line bg-transparent",
        state === "unavailable" && "border-dashed border-warn/50 bg-warn/5",
      )}
    >
      <div className="flex items-center gap-2.5">
        {state === "measured" ? (
          <CheckCircle2 className="h-4 w-4 shrink-0 text-ink-muted" />
        ) : state === "zero" ? (
          <Equal className="h-4 w-4 shrink-0 text-ink-faint" />
        ) : (
          <CircleDashed className="h-4 w-4 shrink-0 text-warn" />
        )}
        <span className="text-sm text-ink">{NAMES[c.name] ?? c.name}</span>
        {state === "measured" ? <Badge>已测得</Badge> : state === "zero" ? <Badge>已测得为零</Badge> : <Badge tone="warn">不可得</Badge>}
        <span
          className={cx(
            "ml-auto font-mono text-sm tabular-nums",
            state === "measured" ? "font-semibold text-ink" : state === "zero" ? "text-ink-muted" : "text-warn",
          )}
        >
          {c.amount === null ? "—" : c.amount === 0 ? "0" : fmt(c.amount)}
        </span>
      </div>
      <div className="mt-1 pl-6.5 text-xs text-ink-faint">
        {state === "unavailable" && c.unavailable ? <span className="text-ink-muted">{c.unavailable}</span> : null}
        {state === "unavailable" && c.unavailable ? " · " : null}
        {nature}
      </div>
    </div>
  );
}

function Legend() {
  const item = (swatch: ReactNode, label: string) => (
    <span className="inline-flex items-center gap-1.5">
      {swatch}
      {label}
    </span>
  );
  return (
    <span className="hidden flex-wrap gap-3 md:flex">
      {item(<span className="h-2.5 w-2.5 rounded-sm bg-ink-muted" />, "已测得")}
      {item(<span className="h-0.5 w-3 bg-ink" />, "为零")}
      {item(<span className="h-2.5 w-2.5 rounded-sm border border-dashed border-warn" />, "不可得")}
      {item(<span className="h-2.5 w-2.5 rounded-sm border border-warn bg-[repeating-linear-gradient(45deg,var(--color-warn)_0_2px,transparent_2px_4px)]" />, "未知")}
    </span>
  );
}

/**
 * Model → each cause → residual → live, as steps. A cause not measured
 * is drawn as a dashed empty step at the level where it would be; an
 * unknown residual is a hatched block labelled 未知, never a zero-height
 * bar.
 */
function Waterfall({ r }: { r: AttributionReport }) {
  const steps = useMemo(() => {
    const out: { label: string; from: number; to: number; kind: "end" | "measured" | "zero" | "unavailable" | "unknown" }[] = [];
    out.push({ label: "模型", from: 0, to: r.model_pnl, kind: "end" });
    let level = r.model_pnl;
    for (const c of r.components) {
      if (c.amount === null) out.push({ label: NAMES[c.name] ?? c.name, from: level, to: level, kind: "unavailable" });
      else {
        out.push({ label: NAMES[c.name] ?? c.name, from: level, to: level + c.amount, kind: c.amount === 0 ? "zero" : "measured" });
        level += c.amount;
      }
    }
    if (r.residual === null) out.push({ label: "残差", from: level, to: r.live_pnl, kind: "unknown" });
    else {
      out.push({ label: "残差", from: level, to: level + r.residual, kind: r.residual === 0 ? "zero" : "measured" });
    }
    out.push({ label: "实盘", from: 0, to: r.live_pnl, kind: "end" });
    return out;
  }, [r]);

  const values = steps.flatMap((s) => [s.from, s.to]);
  const lo = Math.min(0, ...values);
  const hi = Math.max(0, ...values);
  const span = hi - lo || 1;
  const W = 760,
    H = 200,
    pad = 28,
    bw = (W - pad * 2) / steps.length;
  const y = (v: number) => pad + ((hi - v) / span) * (H - pad * 2);

  return (
    <svg viewBox={`0 0 ${W} ${H + 22}`} className="w-full" role="img" aria-label="归因瀑布图">
      <defs>
        <pattern id="attribution-hatch" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
          <line x1="0" y1="0" x2="0" y2="6" stroke="var(--color-warn)" strokeWidth="2" strokeOpacity="0.7" />
        </pattern>
      </defs>
      <line x1={pad} x2={W - pad} y1={y(0)} y2={y(0)} stroke="var(--color-line-strong)" />
      {steps.map((s, i) => {
        const x = pad + i * bw + bw * 0.18;
        const w = bw * 0.64;
        const top = Math.min(y(s.from), y(s.to));
        const h = Math.max(Math.abs(y(s.from) - y(s.to)), 1);
        const next = steps[i + 1];
        // A thin connector from where this step ends to where the next begins.
        const joinY = s.kind === "end" && i > 0 ? null : y(s.to);
        const tag =
          s.kind === "unknown" ? "未知" : s.kind === "unavailable" ? "不可得" : s.kind === "zero" ? "0" : fmt(s.kind === "end" ? s.to : s.to - s.from);
        return (
          <g key={s.label + i}>
            {next && joinY !== null && (
              <line x1={x + w} x2={x + bw} y1={joinY} y2={joinY} stroke="var(--color-line-strong)" strokeDasharray="2 2" />
            )}
            {s.kind === "unavailable" ? (
              <rect x={x} y={y(s.from) - 9} width={w} height={18} rx={3} fill="none" stroke="var(--color-warn)" strokeDasharray="4 3" />
            ) : s.kind === "unknown" ? (
              <rect x={x} y={Math.min(top, y(s.from) - 9)} width={w} height={Math.max(h, 18)} rx={3} fill="url(#attribution-hatch)" stroke="var(--color-warn)" />
            ) : s.kind === "zero" ? (
              <line x1={x} x2={x + w} y1={y(s.from)} y2={y(s.from)} stroke="var(--color-ink)" strokeWidth="2" />
            ) : (
              <rect x={x} y={top} width={w} height={h} rx={2} fill={s.kind === "end" ? "var(--color-accent)" : "var(--color-ink-muted)"} fillOpacity={s.kind === "end" ? 0.85 : 0.7} />
            )}
            <text
              x={x + w / 2}
              y={Math.min(top, y(s.from) - 9) - 6}
              textAnchor="middle"
              fontSize="10.5"
              className="tabular-nums"
              fill={s.kind === "unknown" || s.kind === "unavailable" ? "var(--color-warn)" : "var(--color-ink-muted)"}
            >
              {tag}
            </text>
            <text x={x + w / 2} y={H + 14} textAnchor="middle" fontSize="11" fill="var(--color-ink-muted)">
              {s.label}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
