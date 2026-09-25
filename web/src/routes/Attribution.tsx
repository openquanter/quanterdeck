import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { api, type AttributionReport, type RunEntry } from "@/api/client";
import { Empty, ErrorState, Expert, Skeleton, Term } from "@/components/States";

/**
 * Live minus model, decomposed (UI-BRIEF §4.5).
 *
 * Three states per cause that must never look alike: measured and
 * non-zero, measured and zero, and not measured. And one rule above all:
 * a residual that could not be computed is **unknown**, never 0 — a zero
 * residual from an incomplete decomposition claims everything was
 * explained, which is the one lie this product must not tell.
 */
export function Attribution() {
  const caps = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const [source, setSource] = useState<"shadow" | "runs">("shadow");
  useEffect(() => {
    if (caps.data && !caps.data.ops.available && caps.data.runs.available) setSource("runs");
  }, [caps.data]);

  return (
    <div className="space-y-4">
      <h1 className="text-lg text-ink">归因</h1>
      <div className="flex gap-2 text-sm">
        <SourceTab on={source === "shadow"} onClick={() => setSource("shadow")} disabled={!caps.data?.ops.available}>
          交易进程实时（shadow）
        </SourceTab>
        <SourceTab on={source === "runs"} onClick={() => setSource("runs")} disabled={!caps.data?.runs.available}>
          两份 run 文件
        </SourceTab>
      </div>
      <p className="text-xs text-ink-muted">
        {source === "shadow"
          ? "来源最强：交易进程自己的 shadow 回测和实盘成交，带每笔成交时的市价，滑点和延迟可以分开算。"
          : "来源较弱：run 文件没有成交时的市价，滑点和延迟会是「不可得」。费用和资金费需要你提供交易所账单上的数。"}
      </p>
      {source === "shadow" ? <ShadowSource /> : <RunsSource />}
    </div>
  );
}

function SourceTab({
  on,
  disabled,
  onClick,
  children,
}: {
  on: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      disabled={disabled}
      onClick={onClick}
      className={[
        "rounded border px-3 py-1",
        on ? "border-accent text-accent" : "border-line text-ink-muted",
        disabled ? "opacity-40" : "hover:text-ink",
      ].join(" ")}
    >
      {children}
    </button>
  );
}

function ShadowSource() {
  const q = useQuery({ queryKey: ["ops", "attribution"], queryFn: api.opsAttribution, refetchInterval: 60_000 });
  if (q.isLoading) return <Skeleton tiles={3} rows={5} />;
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
  const input = "rounded border border-line bg-ground p-1.5 font-mono text-xs";
  return (
    <div className="space-y-4">
      <div className="grid gap-3 text-xs text-ink-muted sm:grid-cols-2">
        <label>
          实盘 run
          <select className={`${input} mt-1 w-full`} value={live} onChange={(e) => setLive(e.target.value)}>
            {readable.map((r) => (
              <option key={r.id}>{r.id}</option>
            ))}
          </select>
        </label>
        <label>
          模型 run
          <select className={`${input} mt-1 w-full`} value={model} onChange={(e) => setModel(e.target.value)}>
            <option value="">选择…</option>
            {readable.map((r) => (
              <option key={r.id}>{r.id}</option>
            ))}
          </select>
        </label>
        <label>
          价格精度（小数位，必填）
          <input className={`${input} mt-1 w-24`} value={priceScale} onChange={(e) => setPriceScale(e.target.value.replace(/\D/g, ""))} />
        </label>
        <label>
          数量精度（小数位，必填）
          <input className={`${input} mt-1 w-24`} value={qtyScale} onChange={(e) => setQtyScale(e.target.value.replace(/\D/g, ""))} />
        </label>
        <Pair label="手续费（交易所账单 / 模型）" value={fees} onChange={setFees} className={input} />
        <Pair label="资金费（交易所账单 / 模型）" value={funding} onChange={setFunding} className={input} />
      </div>
      <p className="text-xs text-ink-muted">
        精度没有默认值：填错会让每个成因差一个数量级。手续费和资金费要成对填；只填一半等于没填。
      </p>
      {!ready ? null : q.isLoading ? <Skeleton tiles={3} rows={5} /> : q.isError ? <ErrorState error={q.error} what="归因" /> : q.data ? <Report r={q.data} /> : null}
    </div>
  );
}

function Pair({
  label,
  value,
  onChange,
  className,
}: {
  label: string;
  value: { venue: string; model: string };
  onChange: (v: { venue: string; model: string }) => void;
  className: string;
}) {
  const num = (s: string) => s.replace(/[^\d.-]/g, "");
  return (
    <label>
      {label}
      <div className="mt-1 flex gap-2">
        <input className={`${className} w-28`} placeholder="交易所" value={value.venue} onChange={(e) => onChange({ ...value, venue: num(e.target.value) })} />
        <input className={`${className} w-28`} placeholder="模型" value={value.model} onChange={(e) => onChange({ ...value, model: num(e.target.value) })} />
      </div>
    </label>
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

function Report({ r }: { r: AttributionReport }) {
  const unknownResidual = r.residual === null;
  const missing = r.components.filter((c) => c.amount === null);
  return (
    <div className="space-y-4">
      <div className="grid gap-3 sm:grid-cols-4">
        <Box label="实盘盈亏" value={fmt(r.live_pnl)} />
        <Box label="模型盈亏" value={fmt(r.model_pnl)} />
        <Box label="差额（实盘 − 模型）" value={fmt(r.gap)} />
        <Box
          label={<Term name="residual">残差</Term>}
          value={unknownResidual ? "未知" : fmt(r.residual as number)}
          warn={unknownResidual}
          sub={
            unknownResidual
              ? `有 ${missing.length} 个成因没测到，残差无法计算——不是 0`
              : r.residual_share !== null
                ? `占实盘盈亏 ${(r.residual_share * 100).toFixed(1)}%`
                : undefined
          }
        />
      </div>

      <Waterfall r={r} />

      <table className="w-full text-sm">
        <thead className="text-left text-xs text-ink-muted">
          <tr>
            <th className="py-1 font-normal">成因</th>
            <th className="font-normal">状态</th>
            <th className="text-right font-normal">金额</th>
            <Expert>
              <th className="pl-4 font-normal">性质</th>
            </Expert>
          </tr>
        </thead>
        <tbody>
          {r.components.map((c) => {
            const state = c.amount === null ? "unavailable" : c.amount === 0 ? "zero" : "measured";
            return (
              <tr key={c.name} className="border-t border-line align-top">
                <td className="py-2">{NAMES[c.name] ?? c.name}</td>
                <td>
                  {state === "unavailable" ? (
                    <span className="rounded border border-dashed border-warn px-1.5 text-xs text-warn">不可得</span>
                  ) : state === "zero" ? (
                    <span className="rounded border border-line px-1.5 text-xs text-ink">已测得为零</span>
                  ) : (
                    <span className="rounded bg-surface-raised px-1.5 text-xs text-ink">已测得</span>
                  )}
                  {c.unavailable && <div className="mt-1 text-xs text-ink-muted">{c.unavailable}</div>}
                </td>
                <td className="text-right font-mono tabular-nums">
                  {c.amount === null ? <span className="text-warn">—</span> : fmt(c.amount)}
                </td>
                <Expert>
                  <td className="pl-4 text-xs text-ink-muted">{c.observed ? "两个观测量之差" : "需要判断价值"}</td>
                </Expert>
              </tr>
            );
          })}
        </tbody>
      </table>

      {r.missing_inputs.length > 0 && (
        <section className="rounded border border-warn/50 bg-warn/10 p-3 text-sm">
          <h2 className="mb-1 text-ink">要看到完整答案，还缺：</h2>
          <ul className="list-disc space-y-1 pl-5 text-ink">
            {r.missing_inputs.map((m) => (
              <li key={m}>{m}</li>
            ))}
          </ul>
        </section>
      )}

      <p className="text-xs text-ink-muted">
        来源 {r.method === "shadow" ? "交易进程 shadow（实时）" : "run 文件"} · 匹配成交 {r.matched_fills} · 单边成交{" "}
        {r.unmatched_fills}
        {r.live_run && ` · ${r.live_run} 对 ${r.model_run}`}
      </p>
    </div>
  );
}

function Box({ label, value, sub, warn }: { label: React.ReactNode; value: string; sub?: string; warn?: boolean }) {
  return (
    <div className={`rounded border p-3 ${warn ? "border-warn bg-warn/10" : "border-line bg-surface"}`}>
      <div className="text-xs text-ink-muted">{label}</div>
      <div className={`mt-1 font-mono text-base tabular-nums ${warn ? "text-warn" : "text-ink"}`}>{value}</div>
      {sub && <div className="mt-1 text-xs text-ink-muted">{sub}</div>}
    </div>
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
  const W = 760, H = 180, pad = 24, bw = (W - pad * 2) / steps.length;
  const y = (v: number) => pad + ((hi - v) / span) * (H - pad * 2);

  return (
    <svg viewBox={`0 0 ${W} ${H + 20}`} className="w-full rounded border border-line bg-surface" role="img" aria-label="归因瀑布图">
      <defs>
        <pattern id="hatch" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
          <line x1="0" y1="0" x2="0" y2="6" stroke="var(--color-warn)" strokeWidth="2" />
        </pattern>
      </defs>
      <line x1={pad} x2={W - pad} y1={y(0)} y2={y(0)} stroke="var(--color-line)" />
      {steps.map((s, i) => {
        const x = pad + i * bw + bw * 0.15;
        const w = bw * 0.7;
        const top = Math.min(y(s.from), y(s.to));
        const h = Math.max(Math.abs(y(s.from) - y(s.to)), 1);
        return (
          <g key={s.label + i}>
            {s.kind === "unavailable" ? (
              <rect x={x} y={y(s.from) - 8} width={w} height={16} fill="none" stroke="var(--color-warn)" strokeDasharray="4 3" />
            ) : s.kind === "unknown" ? (
              <rect x={x} y={Math.min(top, y(s.from) - 8)} width={w} height={Math.max(h, 16)} fill="url(#hatch)" stroke="var(--color-warn)" />
            ) : s.kind === "zero" ? (
              <line x1={x} x2={x + w} y1={y(s.from)} y2={y(s.from)} stroke="var(--color-ink)" strokeWidth="2" />
            ) : (
              <rect x={x} y={top} width={w} height={h} fill={s.kind === "end" ? "var(--color-accent)" : "var(--color-ink-muted)"} />
            )}
            <text x={x + w / 2} y={H + 12} textAnchor="middle" fontSize="11" fill="var(--color-ink-muted)">
              {s.label}
            </text>
            {(s.kind === "unavailable" || s.kind === "unknown") && (
              <text x={x + w / 2} y={Math.min(top, y(s.from)) - 12} textAnchor="middle" fontSize="10" fill="var(--color-warn)">
                {s.kind === "unknown" ? "未知" : "不可得"}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}
