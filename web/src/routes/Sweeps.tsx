import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";

import { api, type Sweep } from "@/api/client";
import { Empty, ErrorState, Skeleton, Term } from "@/components/States";

import { Tile } from "./Ops";

/**
 * Parameter sweeps (UI-BRIEF /sweeps): every configuration's outcome with
 * the overfitting statistics beside it, and the verdict first. A table of
 * winners on its own is what a sweep exists to refuse, so the table never
 * appears without whether the search can be trusted above it.
 */
export function Sweeps() {
  const [params, setParams] = useSearchParams();
  const selected = params.get("id");
  const list = useQuery({ queryKey: ["sweeps"], queryFn: api.sweeps });

  return (
    <div className="space-y-4">
      <h1 className="text-lg text-ink">参数扫描</h1>
      <p className="text-xs text-ink-muted">
        策略是编译进程序的 Rust 代码，扫描在调用方自己的程序里跑，结果写成 <code className="font-mono">.sweep</code>{" "}
        文件放在运行记录目录（例如 <code className="font-mono">sweep_100 --out 文件</code>）。这里只读这些文件。
      </p>
      {list.isLoading ? (
        <Skeleton rows={4} />
      ) : list.isError ? (
        <ErrorState error={list.error} what="扫描结果" />
      ) : !list.data?.length ? (
        <Empty title="运行记录目录里还没有 .sweep 文件。" next="跑一次参数扫描并用 --out 把结果写到运行记录目录。" />
      ) : (
        <div className="flex flex-wrap gap-2">
          {list.data.map((e) => (
            <button
              key={e.id}
              onClick={() => setParams({ id: e.id })}
              className={`rounded border px-3 py-1.5 text-left text-xs ${selected === e.id ? "border-accent" : "border-line"}`}
            >
              <div className="font-mono text-ink">{e.id}</div>
              {e.state === "read" ? (
                <div className="text-ink-muted">
                  {e.label} · {e.configs} 组 ·{" "}
                  <span className={e.refused ? "text-bad" : "text-good"}>{e.refused ? "不可部署" : "未被拒绝"}</span>
                </div>
              ) : (
                <div className="text-warn">读不出：{e.error}</div>
              )}
            </button>
          ))}
        </div>
      )}
      {selected && <Detail id={selected} />}
    </div>
  );
}

function Detail({ id }: { id: string }) {
  const q = useQuery({ queryKey: ["sweep", id], queryFn: () => api.sweep(id) });
  if (q.isLoading) return <Skeleton tiles={4} rows={6} />;
  if (q.isError) return <ErrorState error={q.error} what={`扫描 ${id}`} />;
  return <SweepView s={q.data!} />;
}

function SweepView({ s }: { s: Sweep }) {
  const t = s.thresholds;
  const dsr = s.deflated_sharpe;
  const pbo = s.pbo;
  return (
    <div className="space-y-4">
      {/* The verdict before the table. */}
      {s.refusals.length > 0 ? (
        <section className="rounded border border-bad/60 bg-bad/10 p-3">
          <h2 className="mb-1 text-sm text-ink">不可部署：{s.refusals.length} 条理由</h2>
          <ul className="space-y-1 text-sm">
            {s.refusals.map((r, k) => (
              <li key={k}>
                <div className="text-ink">{translate(r)}</div>
                <div className="text-xs text-ink-muted">{r}</div>
              </li>
            ))}
          </ul>
        </section>
      ) : (
        <p className="rounded border border-good/60 bg-good/10 p-3 text-sm text-ink">
          没有被拒绝：过拟合概率、折减夏普和样本外退化都在阈值内，也没有发现使用未来数据。这只说明这次搜索没有明显的自欺，不代表实盘会赚钱。
        </p>
      )}

      <div className="grid gap-3 sm:grid-cols-4">
        <Tile
          label={<Term name="dsr">折减夏普</Term>}
          value={dsr.state === "value" ? dsr.value.toFixed(3) : "无法计算"}
          sub={dsr.state === "value" ? `至少 ${t.min_deflated_sharpe}` : dsr.reason}
          tone={dsr.state === "value" && dsr.value >= t.min_deflated_sharpe ? "good" : "bad"}
        />
        <Tile
          label={<Term name="pbo">回测过拟合概率</Term>}
          value={pbo.state === "value" ? pbo.value.pbo.toFixed(3) : "无法计算"}
          sub={pbo.state === "value" ? `至多 ${t.max_pbo} · ${pbo.value.splits} 次切分` : pbo.reason}
          tone={pbo.state === "value" && pbo.value.pbo <= t.max_pbo ? "good" : "bad"}
        />
        <Tile
          label={<Term name="degradation">样本外退化斜率</Term>}
          value={pbo.state === "value" ? pbo.value.degradation.toFixed(3) : "—"}
          sub={`需大于 ${t.min_degradation_slope}`}
          tone={pbo.state === "value" ? (pbo.value.degradation > t.min_degradation_slope ? "good" : "bad") : undefined}
        />
        <Tile
          label="样本外"
          value={pbo.state === "value" ? `亏损概率 ${(pbo.value.probability_of_loss * 100).toFixed(0)}%` : "—"}
          sub={pbo.state === "value" ? `样本外夏普中位数 ${pbo.value.median_oos_sharpe.toFixed(3)}` : undefined}
        />
      </div>

      {pbo.state === "value" && pbo.value.logits.length > 0 && <Logits logits={pbo.value.logits} />}

      <Configs s={s} />

      {s.unscorable.length > 0 && (
        <section className="text-xs">
          <h2 className="mb-1 text-sm text-ink-muted">无法评分的参数组（{s.unscorable.length}）</h2>
          <p className="mb-1 text-ink-muted">收益序列太短，算不出夏普；它们不参与过拟合统计。</p>
          <ul className="font-mono">
            {s.unscorable.map((u) => (
              <li key={u}>{u}</li>
            ))}
          </ul>
        </section>
      )}
      <p className="text-xs text-ink-muted">
        未来数据检查：
        {s.lookahead ? (
          <span className="font-mono">
            {s.lookahead[0]} — {s.lookahead[1]}
          </span>
        ) : (
          "这次扫描没有做（只有给了检查用的策略工厂时才做）。"
        )}{" "}
        · 权益每 {s.equity_every} 个 tick 采样一次
      </p>
    </div>
  );
}

/** Where the in-sample winner landed out of sample, split by split. */
function Logits({ logits }: { logits: number[] }) {
  const bins = useMemo(() => {
    // Bins aligned on 0, so no bin straddles the line that matters.
    const width = Math.max((Math.ceil(Math.max(...logits, 1)) - Math.floor(Math.min(...logits, -1))) / 20, 0.1);
    const lo = Math.floor(Math.min(...logits, -1) / width) * width;
    const hi = Math.ceil(Math.max(...logits, 1) / width) * width;
    const counts = new Map<number, number>();
    for (const l of logits) {
      const b = Math.floor(l / width) * width;
      counts.set(b, (counts.get(b) ?? 0) + 1);
    }
    return { lo, hi, width, counts: [...counts.entries()].sort((a, b) => a[0] - b[0]) };
  }, [logits]);
  const max = Math.max(...bins.counts.map((c) => c[1]));
  const W = 600, H = 100;
  const x = (v: number) => ((v - bins.lo) / (bins.hi - bins.lo)) * W;
  const below = logits.filter((l) => l <= 0).length;
  return (
    <section className="rounded border border-line bg-surface p-3">
      <h2 className="mb-1 text-sm text-ink-muted">
        <Term name="logit">样本内最优在样本外的排名</Term>：{below} / {logits.length} 次落到后一半（0 线左侧）
      </h2>
      <svg viewBox={`0 0 ${W} ${H + 14}`} className="h-32 w-full" preserveAspectRatio="none">
        {bins.counts.map(([b, c]) => (
          <rect
            key={b}
            x={x(b) + 1}
            y={H - (c / max) * H}
            width={Math.max(x(b + bins.width) - x(b) - 2, 1)}
            height={(c / max) * H}
            fill={b + bins.width <= 0 ? "var(--color-warn)" : "var(--color-accent)"}
            opacity={0.8}
          />
        ))}
        <line x1={x(0)} x2={x(0)} y1={0} y2={H} stroke="var(--color-ink)" strokeWidth={1} vectorEffect="non-scaling-stroke" />
        <text x={x(0) + 4} y={H + 12} fontSize={10} fill="var(--color-ink-muted)">
          0
        </text>
      </svg>
    </section>
  );
}

type Key = "sharpe" | "realized" | "final_equity" | "min_equity" | "fills" | "label";

function Configs({ s }: { s: Sweep }) {
  const [key, setKey] = useState<Key>("sharpe");
  const rows = useMemo(() => {
    const v = (c: Sweep["configs"][number]) => (key === "label" ? 0 : (c[key] ?? -Infinity));
    return [...s.configs].sort((a, b) => (key === "label" ? a.label.localeCompare(b.label) : v(b) - v(a)));
  }, [s.configs, key]);
  const head = (k: Key, label: string, right = true) => (
    <th className={`px-2 py-1 font-normal ${right ? "text-right" : ""}`}>
      <button className={key === k ? "text-accent" : "hover:text-ink"} onClick={() => setKey(k)}>
        {label}
        {key === k && " ↓"}
      </button>
    </th>
  );
  return (
    <section>
      <h2 className="mb-1 text-sm text-ink-muted">
        各参数组（{s.configs.length}）· 样本内结果。排第一的正是上面过拟合统计在检验的那一组。
      </h2>
      <div className="max-h-[32rem] overflow-auto rounded border border-line">
        <table className="w-full text-sm">
          <thead className="sticky top-0 bg-surface text-left text-xs text-ink-muted">
            <tr>
              {head("label", "参数", false)}
              {head("sharpe", "夏普（逐采样）")}
              {head("realized", "已实现")}
              <th className="px-2 py-1 text-right font-normal">手续费</th>
              {head("final_equity", "期末权益")}
              {head("min_equity", "最低权益")}
              {head("fills", "成交")}
              <th className="px-2 py-1 text-right font-normal">强平</th>
            </tr>
          </thead>
          <tbody className="font-mono text-xs">
            {rows.map((c) => (
              <tr key={c.label} className="border-t border-line">
                <td className="px-2 py-1">{c.label}</td>
                <td className="px-2 py-1 text-right">{c.sharpe === null ? "—" : sig(c.sharpe)}</td>
                <td className="px-2 py-1 text-right">{c.realized.toFixed(2)}</td>
                <td className="px-2 py-1 text-right">{c.fees.toFixed(2)}</td>
                <td className="px-2 py-1 text-right">{c.final_equity.toFixed(2)}</td>
                <td className="px-2 py-1 text-right">{c.min_equity.toFixed(2)}</td>
                <td className="px-2 py-1 text-right">{c.fills}</td>
                <td className={`px-2 py-1 text-right ${c.liquidations > 0 ? "text-bad" : ""}`}>{c.liquidations}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

/** Per-sample Sharpe ratios are small; fixed decimals print them all as 0. */
function sig(v: number) {
  return v === 0 ? "0" : v.toPrecision(3);
}

/** The framework's refusals are English sentences; the operator reads Chinese. The original stays beneath. */
function translate(r: string) {
  let m = r.match(/^probability of backtest overfitting is ([\d.]+), above the limit of ([\d.]+)/);
  if (m) return `回测过拟合概率 ${m[1]}，高于上限 ${m[2]}：这次搜索找到的更可能是噪声而不是优势。`;
  m = r.match(/^out-of-sample Sharpe regressed on in-sample Sharpe has slope (-?[\d.]+), at or below (-?[\d.]+)/);
  if (m) return `样本外夏普对样本内夏普的斜率 ${m[1]}，不高于 ${m[2]}：样本内的排名对样本外毫无预测力，第一名只是最贴合噪声的那组。`;
  m = r.match(/^deflated Sharpe ratio is (-?[\d.]+), below the limit of ([\d.]+)/);
  if (m) return `折减夏普 ${m[1]}，低于下限 ${m[2]}：考虑到试了这么多组，这个结果站不住。`;
  m = r.match(/^(.+) decides differently on a prefix of the data than on all of it, first at tick (\d+)/);
  if (m) return `${m[1]} 只看前一段数据和看全部数据时，在第 ${m[2]} 个 tick 做出了不同决定：它用到了当时不可能有的数据，结果在实盘无法复现。`;
  m = r.match(/^(.+) could not be computed \((.*)\); a sweep/);
  if (m) return `${m[1]} 无法计算（${m[2]}）：没法评分的扫描不等于评分合格。`;
  return r;
}
