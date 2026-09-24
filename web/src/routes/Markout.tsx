import { useQuery } from "@tanstack/react-query";
import { BarChart, CustomChart } from "echarts/charts";
import { GridComponent, LegendComponent, TooltipComponent } from "echarts/components";
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import ReactEChartsCore from "echarts-for-react/lib/core";
import { useMemo, useState } from "react";

import { api, type Horizon, type MarkoutComparison } from "@/api/client";

import { Failure } from "./Runs";

// Only what this chart draws. The full build is a megabyte, most of it
// chart types nobody here uses.
echarts.use([BarChart, CustomChart, GridComponent, LegendComponent, TooltipComponent, CanvasRenderer]);

/** A colour from the design tokens, read where the chart needs it. */
function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(`--color-${name}`).trim();
}

/**
 * Where the price went after each fill, for the two runs being compared.
 *
 * The tick file is the operator's choice and is shown with the result:
 * a run records what it traded and not the market it traded in, and a
 * run priced against another day's ticks is a comparison of nothing.
 * A horizon with too few fills says so rather than drawing a bar.
 */
export function MarkoutPanel({ baseline, candidate }: { baseline: string; candidate: string }) {
  const { data: caps } = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const available = caps?.markout.available ?? false;
  const { data: files } = useQuery({ queryKey: ["ticks"], queryFn: api.ticks, enabled: available });
  const [ticks, setTicks] = useState("");

  const { data, error, isFetching } = useQuery({
    queryKey: ["markout", baseline, candidate, ticks],
    queryFn: () => api.markout(baseline, candidate, ticks),
    enabled: available && ticks !== "" && baseline !== "" && candidate !== "",
  });

  if (caps && !available) {
    return (
      <section className="mt-8 text-sm">
        <h2 className="mb-2 text-ink">成交后价格走势（markout）</h2>
        <p className="text-ink-muted">{caps.markout.reason}</p>
      </section>
    );
  }

  return (
    <section className="mt-8 text-sm">
      <h2 className="mb-2 text-ink">成交后价格走势（markout）</h2>
      <p className="mb-3 text-ink-muted">
        每笔成交之后价格朝有利方向走了多少个基点。实盘比回测更负，说明回测没模拟到的逆向选择。
      </p>
      <label className="mb-4 flex items-center gap-2">
        <span className="text-ink-muted">定价用的 tick 文件</span>
        <select
          value={ticks}
          onChange={(event) => setTicks(event.target.value)}
          className="rounded border border-line bg-ground px-2 py-1 font-mono text-sm text-ink"
        >
          <option value="">—</option>
          {(files ?? []).map((id) => (
            <option key={id} value={id}>
              {id}
            </option>
          ))}
        </select>
      </label>
      {error && <Failure error={error} />}
      {isFetching && <p className="text-ink-muted">计算中…</p>}
      {data && <MarkoutResult data={data} />}
    </section>
  );
}

function MarkoutResult({ data }: { data: MarkoutComparison }) {
  const option = useMemo(() => {
    const labels = data.baseline.horizons.map((h) => `${h.seconds}s`);
    const series = (name: string, horizons: Horizon[], colour: string) => ({
      name,
      type: "bar" as const,
      itemStyle: { color: colour },
      // A horizon with too few fills is a gap, not a zero bar.
      data: horizons.map((h) => (h.measured ? h.mean_bps : null)),
    });
    const band = (horizons: Horizon[], offset: number) => ({
      type: "custom" as const,
      silent: true,
      renderItem: (
        _: unknown,
        api: {
          value: (i: number) => number;
          coord: (p: [number, number]) => [number, number];
          size: (p: [number, number]) => [number, number];
          style: (o: object) => object;
        },
      ) => {
        const x = api.value(0);
        const low = api.coord([x, api.value(1)]);
        const high = api.coord([x, api.value(2)]);
        const width = api.size([1, 0])[0] * 0.2;
        const cx = low[0] + offset * width;
        return {
          type: "line",
          shape: { x1: cx, y1: low[1], x2: cx, y2: high[1] },
          style: { stroke: token("ink-muted"), lineWidth: 1 },
        };
      },
      data: horizons.flatMap((h, i) =>
        h.measured ? [[i, h.p10_bps ?? 0, h.p90_bps ?? 0]] : [],
      ),
    });
    return {
      backgroundColor: "transparent",
      textStyle: { color: token("ink-muted") },
      tooltip: { trigger: "axis" },
      legend: { textStyle: { color: token("ink-muted") } },
      xAxis: { type: "category", data: labels, axisLine: { lineStyle: { color: token("line") } } },
      yAxis: {
        type: "value",
        name: "bp",
        splitLine: { lineStyle: { color: token("line") } },
      },
      series: [
        series(`基准 ${data.baseline.id}`, data.baseline.horizons, token("accent")),
        series(`待测 ${data.candidate.id}`, data.candidate.horizons, token("warn")),
        band(data.baseline.horizons, -1),
        band(data.candidate.horizons, 1),
      ],
    };
  }, [data]);

  return (
    <>
      <ReactEChartsCore echarts={echarts} option={option} style={{ height: 280 }} notMerge />
      <table className="mt-4 w-full font-mono text-xs">
        <thead className="text-ink-muted">
          <tr>
            <th className="text-left">期限</th>
            <th className="text-right">基准 均值 / 不利占比 / 笔数</th>
            <th className="text-right">待测 均值 / 不利占比 / 笔数</th>
            <th className="text-right">差（待测 − 基准）</th>
          </tr>
        </thead>
        <tbody>
          {data.contrast.map((c, i) => (
            <tr key={c.seconds}>
              <td>{c.seconds}s</td>
              <td className="text-right">{describe(data.baseline.horizons[i])}</td>
              <td className="text-right">{describe(data.candidate.horizons[i])}</td>
              <td className="text-right">
                {c.difference_bps === null
                  ? "无法判断：一侧样本不足"
                  : `${c.difference_bps >= 0 ? "+" : ""}${c.difference_bps.toFixed(2)} bp`}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="mt-2 text-ink-muted">价格来自 {data.ticks}；竖线为 10%–90% 分位。</p>
    </>
  );
}

function describe(h: Horizon | undefined): string {
  if (!h) return "—";
  if (!h.measured) return `样本不足（${h.samples} 笔）`;
  const mean = h.mean_bps ?? 0;
  return `${mean >= 0 ? "+" : ""}${mean.toFixed(2)} bp / ${((h.adverse_share ?? 0) * 100).toFixed(0)}% / ${h.samples}`;
}
