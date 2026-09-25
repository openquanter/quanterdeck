import { useQuery } from "@tanstack/react-query";
import { BarChart, CustomChart } from "echarts/charts";
import { GridComponent, LegendComponent, TooltipComponent } from "echarts/components";
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import ReactEChartsCore from "echarts-for-react/lib/core";
import { useMemo, useState } from "react";

import { api, type Horizon, type MarkoutComparison } from "@/api/client";
import { ErrorState, Skeleton, Term } from "@/components/States";
import { SERIES } from "@/ui/charts";
import { Card, Table, cx } from "@/ui/kit";

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

  const title = (
    <>
      成交后价格走势（<Term name="markout">markout</Term>）
    </>
  );

  if (caps && !available) {
    return (
      <Card title={title}>
        <p className="text-sm text-ink-muted">{caps.markout.reason}</p>
      </Card>
    );
  }

  return (
    <Card
      title={title}
      extra={
        <label className="flex items-center gap-2">
          <span>定价用的 tick 文件</span>
          <select
            value={ticks}
            onChange={(event) => setTicks(event.target.value)}
            className="h-7 rounded-md border border-line-strong bg-surface-raised px-2 font-mono text-xs text-ink"
          >
            <option value="">—</option>
            {(files ?? []).map((id) => (
              <option key={id} value={id}>
                {id}
              </option>
            ))}
          </select>
        </label>
      }
    >
      <p className="text-sm text-ink-muted">每笔成交之后价格朝有利方向走了多少个基点。实盘比回测更负，说明回测没模拟到的逆向选择。</p>
      {!ticks && (
        <p className="mt-3 text-sm text-ink-faint">
          先在右上角选一份 tick 文件：run 只记下了它成交了什么，没有记下当时的行情；拿另一天的行情去定价，比较的是空气。
        </p>
      )}
      {error ? (
        <div className="mt-4">
          <ErrorState error={error} what="markout" />
        </div>
      ) : null}
      {isFetching && !data && (
        <div className="mt-4">
          <Skeleton rows={4} />
        </div>
      )}
      {data && <MarkoutResult data={data} />}
    </Card>
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
        const mid = low[0] + offset * width;
        return {
          type: "line",
          shape: { x1: mid, y1: low[1], x2: mid, y2: high[1] },
          style: { stroke: token("ink-muted"), lineWidth: 1 },
        };
      },
      data: horizons.flatMap((h, i) =>
        h.measured && h.p10_bps !== null && h.p90_bps !== null ? [[i, h.p10_bps, h.p90_bps]] : [],
      ),
    });
    return {
      backgroundColor: "transparent",
      textStyle: { color: token("ink-muted") },
      tooltip: {
        trigger: "axis",
        backgroundColor: token("surface-raised"),
        borderColor: token("line-strong"),
        textStyle: { color: token("ink"), fontSize: 12 },
      },
      grid: { left: 8, right: 12, top: 36, bottom: 8, containLabel: true },
      legend: { textStyle: { color: token("ink-muted") } },
      xAxis: { type: "category", data: labels, axisLine: { lineStyle: { color: token("line") } } },
      yAxis: {
        type: "value",
        name: "bp",
        splitLine: { lineStyle: { color: token("line") } },
      },
      series: [
        // Series colours, not state colours: amber would read as a warning.
        series(`基准 ${data.baseline.id}`, data.baseline.horizons, SERIES[0]),
        series(`待测 ${data.candidate.id}`, data.candidate.horizons, SERIES[1]),
        band(data.baseline.horizons, -1),
        band(data.candidate.horizons, 1),
      ],
    };
  }, [data]);

  return (
    <div className="mt-4 space-y-4">
      <ReactEChartsCore echarts={echarts} option={option} style={{ height: 280 }} notMerge />
      <Table
        dense
        head={[
          "期限",
          <span key="b" className="block text-right">基准 均值 / 不利占比 / 笔数</span>,
          <span key="c" className="block text-right">待测 均值 / 不利占比 / 笔数</span>,
          <span key="d" className="block text-right">差（待测 − 基准）</span>,
        ]}
      >
        {data.contrast.map((c, i) => (
          <tr key={c.seconds}>
            <td className="font-mono text-xs">{c.seconds}s</td>
            <td className={cx("text-right font-mono text-xs", !data.baseline.horizons[i]?.measured && "text-warn")}>{describe(data.baseline.horizons[i])}</td>
            <td className={cx("text-right font-mono text-xs", !data.candidate.horizons[i]?.measured && "text-warn")}>{describe(data.candidate.horizons[i])}</td>
            {/* One side too thin to measure is "cannot tell", never a 0 bp difference. */}
            <td className={cx("text-right font-mono text-xs", c.difference_bps === null ? "text-warn" : "text-ink")}>
              {c.difference_bps === null ? "无法判断：一侧样本不足" : `${c.difference_bps >= 0 ? "+" : ""}${c.difference_bps.toFixed(2)} bp`}
            </td>
          </tr>
        ))}
      </Table>
      <p className="text-xs text-ink-faint">
        价格来自 <span className="font-mono">{data.ticks}</span>；竖线为 10%–90% 分位；样本不足的期限不画柱，而不是画成 0。
      </p>
    </div>
  );
}

function describe(h: Horizon | undefined): string {
  if (!h) return "—";
  if (!h.measured) return `样本不足（${h.samples} 笔）`;
  // A statistic the server did not send is unknown, not zero.
  const mean = h.mean_bps === null ? "—" : `${h.mean_bps >= 0 ? "+" : ""}${h.mean_bps.toFixed(2)} bp`;
  const adverse = h.adverse_share === null ? "—" : `${(h.adverse_share * 100).toFixed(0)}%`;
  return `${mean} / ${adverse} / ${h.samples}`;
}
