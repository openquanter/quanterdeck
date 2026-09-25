import { useMemo } from "react";
import { LineChart } from "echarts/charts";
import { DataZoomComponent, GridComponent, LegendComponent, MarkAreaComponent, MarkLineComponent, TooltipComponent } from "echarts/components";
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import ReactEChartsCore from "echarts-for-react/lib/core";

echarts.use([LineChart, GridComponent, TooltipComponent, LegendComponent, DataZoomComponent, MarkLineComponent, MarkAreaComponent, CanvasRenderer]);

/** Series colours, in order. Distinct from the state colours on purpose. */
export const SERIES = ["#5b9bff", "#a78bfa", "#2dd4bf", "#f472b6", "#fbbf24", "#94a3b8"];
const AXIS = "#6b7383";
const GRID = "#232833";

export type Point = [number, number | null];

/**
 * A time series chart with the console's look: hover shows every series
 * at that instant, a gap in the data is a gap in the line, and a click
 * reports the time clicked.
 */
export function TimeSeries({
  series,
  height = 180,
  unit,
  from,
  to,
  marks,
  bands,
  onPick,
  zoom,
  decimals = 2,
}: {
  series: { name: string; points: Point[]; color?: string; area?: boolean }[];
  height?: number;
  unit?: string;
  from?: number;
  to?: number;
  /** Vertical lines: events. */
  marks?: { at: number; label?: string; color?: string }[];
  /** Shaded spans: halts, gaps. */
  bands?: [number, number][];
  onPick?: (ms: number) => void;
  zoom?: boolean;
  decimals?: number;
}) {
  const option = useMemo(() => {
    const fmt = (v: number) => `${Number(v.toFixed(decimals))}${unit ?? ""}`;
    return {
      animation: false,
      grid: { left: 8, right: 12, top: series.length > 1 ? 30 : 12, bottom: zoom ? 44 : 8, containLabel: true },
      legend: series.length > 1 ? { top: 0, left: 0, icon: "roundRect", itemWidth: 10, itemHeight: 3, textStyle: { color: "#9aa3b2", fontSize: 11 } } : undefined,
      tooltip: {
        trigger: "axis",
        backgroundColor: "#191d25",
        borderColor: "#2e3542",
        textStyle: { color: "#e8ebf0", fontSize: 12 },
        valueFormatter: (v: number | null) => (v == null ? "—" : fmt(v)),
        axisPointer: { lineStyle: { color: "#6b7383" } },
      },
      xAxis: {
        type: "time",
        min: from,
        max: to,
        axisLine: { lineStyle: { color: GRID } },
        axisLabel: { color: AXIS, fontSize: 11, hideOverlap: true },
        splitLine: { show: false },
      },
      yAxis: {
        type: "value",
        scale: true,
        axisLabel: { color: AXIS, fontSize: 11, formatter: (v: number) => fmt(v) },
        splitLine: { lineStyle: { color: GRID } },
      },
      dataZoom: zoom ? [{ type: "inside" }, { type: "slider", height: 18, bottom: 8, borderColor: GRID, textStyle: { color: AXIS } }] : undefined,
      series: series.map((s, i) => ({
        name: s.name,
        type: "line",
        showSymbol: false,
        connectNulls: false,
        sampling: "lttb",
        lineStyle: { width: 1.5 },
        color: s.color ?? SERIES[i % SERIES.length],
        areaStyle: s.area ? { opacity: 0.12 } : undefined,
        data: withGaps(s.points),
        markLine:
          i === 0 && marks?.length
            ? { silent: true, symbol: "none", label: { show: false }, data: marks.map((m) => ({ xAxis: m.at, lineStyle: { color: m.color ?? "#e8a83e", type: "dashed", width: 1, opacity: 0.7 } })) }
            : undefined,
        markArea:
          i === 0 && bands?.length
            ? { silent: true, itemStyle: { color: "rgba(239,90,95,0.10)" }, data: bands.map(([a, b]) => [{ xAxis: a }, { xAxis: b }]) }
            : undefined,
      })),
    };
  }, [series, from, to, marks, bands, unit, zoom, decimals]);

  return (
    <ReactEChartsCore
      echarts={echarts}
      option={option}
      notMerge
      style={{ height, cursor: onPick ? "crosshair" : undefined }}
      onEvents={
        onPick
          ? {
              click: (p: { value?: [number, number] }) => p.value && onPick(p.value[0]),
            }
          : undefined
      }
    />
  );
}

/**
 * Insert a null where samples stop for much longer than usual, so a
 * stretch with no recording is drawn as a gap and not as a steady value.
 */
function withGaps(points: Point[]): Point[] {
  if (points.length < 3) return points;
  const steps = points.slice(1).map((p, k) => p[0] - points[k][0]).sort((a, b) => a - b);
  const gap = 3 * steps[Math.floor(steps.length / 2)];
  const out: Point[] = [points[0]];
  for (let k = 1; k < points.length; k++) {
    if (points[k][0] - points[k - 1][0] > gap) out.push([points[k - 1][0] + 1, null]);
    out.push(points[k]);
  }
  return out;
}

/** A tiny trend line for a stat. */
export function Sparkline({ points, color = SERIES[0], height = 36 }: { points: Point[]; color?: string; height?: number }) {
  const option = useMemo(
    () => ({
      animation: false,
      grid: { left: 0, right: 0, top: 2, bottom: 2 },
      xAxis: { type: "time", show: false },
      yAxis: { type: "value", show: false, scale: true },
      series: [{ type: "line", showSymbol: false, data: withGaps(points), color, lineStyle: { width: 1.5 }, areaStyle: { opacity: 0.12 } }],
    }),
    [points, color],
  );
  return <ReactEChartsCore echarts={echarts} option={option} notMerge style={{ height }} />;
}
