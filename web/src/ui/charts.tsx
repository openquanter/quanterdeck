import { useEffect, useMemo, useRef } from "react";
import { LineChart } from "echarts/charts";
import { DataZoomComponent, GridComponent, LegendComponent, MarkAreaComponent, MarkLineComponent, TooltipComponent } from "echarts/components";
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import ReactEChartsCore from "echarts-for-react/lib/core";

echarts.use([LineChart, GridComponent, TooltipComponent, LegendComponent, DataZoomComponent, MarkLineComponent, MarkAreaComponent, CanvasRenderer]);

/** The instance type echarts-for-react hands to `onChartReady`. */
type ChartInstance = Parameters<NonNullable<React.ComponentProps<typeof ReactEChartsCore>["onChartReady"]>>[0];

import { useThemeColors } from "./theme";

/**
 * A theme colour with the alpha given, as `rgba()`.
 *
 * Not `color-mix()`: a canvas can paint it, but the chart library
 * animates between colours by parsing them, and a colour it cannot parse
 * crashed the page the first time a chart redrew with animation on.
 */
function fade(color: string, alpha: number) {
  const hex = color.trim().replace("#", "");
  const full = hex.length === 3 ? hex.replace(/./g, (c) => c + c) : hex;
  const n = Number.parseInt(full, 16);
  if (full.length !== 6 || Number.isNaN(n)) return color;
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

/** A gradient from the line's colour down to nothing. */
function areaFill(color: string) {
  return {
    color: {
      type: "linear",
      x: 0,
      y: 0,
      x2: 0,
      y2: 1,
      colorStops: [
        { offset: 0, color: fade(color, 0.28) },
        { offset: 1, color: fade(color, 0.02) },
      ],
    },
  };
}

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
  levels,
  step,
}: {
  series: { name: string; points: Point[]; color?: string; area?: boolean }[];
  /** Horizontal lines: prices, limits. */
  levels?: { value: number; label?: string; color?: string; dashed?: boolean }[];
  /** Draw as steps: a value that holds until it changes. */
  step?: boolean;
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
  const c = useThemeColors();
  const option = useMemo(() => {
    const fmt = (v: number) => `${Number(v.toFixed(decimals))}${unit ?? ""}`;
    const lines = [
      ...(marks ?? []).map((m) => ({ xAxis: m.at, lineStyle: { color: m.color ?? c.warn, type: "dashed", width: 1, opacity: 0.8 } })),
      ...(levels ?? []).map((l) => ({
        yAxis: l.value,
        label: { show: Boolean(l.label), formatter: l.label ?? "", position: "insideEndTop", color: l.color ?? c.muted, fontSize: 10 },
        lineStyle: { color: l.color ?? c.faint, type: l.dashed === false ? "solid" : "dashed", width: 1, opacity: 0.9 },
      })),
    ];
    return {
      animation: true,
      animationDuration: 300,
      grid: { left: 8, right: 16, top: series.length > 1 ? 34 : 14, bottom: zoom ? 44 : 8, containLabel: true },
      legend:
        series.length > 1
          ? { top: 0, left: 0, icon: "circle", itemWidth: 8, itemHeight: 8, itemGap: 14, textStyle: { color: c.muted, fontSize: 11 } }
          : undefined,
      tooltip: {
        trigger: "axis",
        backgroundColor: c.surface,
        borderColor: c.lineStrong,
        borderWidth: 1,
        padding: [8, 12],
        extraCssText: "border-radius: 12px; box-shadow: 0 4px 16px rgba(0,0,0,0.12);",
        textStyle: { color: c.ink, fontSize: 12 },
        valueFormatter: (v: number | null) => (v == null ? "—" : fmt(v)),
        axisPointer: { type: "line", lineStyle: { color: c.faint, type: "dashed" } },
      },
      xAxis: {
        type: "time",
        min: from,
        max: to,
        axisLine: { lineStyle: { color: c.line } },
        axisTick: { show: false },
        axisLabel: { color: c.faint, fontSize: 11, hideOverlap: true },
        splitLine: { show: false },
      },
      yAxis: {
        type: "value",
        scale: true,
        // Levels are part of what the chart shows: widen the axis to
        // them, or a price line drawn off the axis is silently absent.
        ...(levels?.length
          ? {
              min: (v: { min: number }) => Math.min(v.min, ...levels.map((l) => l.value)),
              max: (v: { max: number }) => Math.max(v.max, ...levels.map((l) => l.value)),
            }
          : {}),
        axisLabel: { color: c.faint, fontSize: 11, formatter: (v: number) => fmt(v) },
        splitLine: { lineStyle: { color: c.line, type: "dashed" } },
      },
      dataZoom: zoom
        ? [
            { type: "inside" },
            { type: "slider", height: 18, bottom: 8, borderColor: c.line, fillerColor: fade(c.accent, 0.12), textStyle: { color: c.faint }, handleStyle: { color: c.accent } },
          ]
        : undefined,
      series: series.map((s, i) => {
        const color = s.color ?? c.series[i % c.series.length];
        return {
          name: s.name,
          type: "line",
          showSymbol: false,
          smooth: !step && series.length === 1 ? 0.25 : false,
          step: step ? "end" : undefined,
          connectNulls: false,
          sampling: "lttb",
          lineStyle: { width: 2 },
          color,
          areaStyle: s.area ? areaFill(color) : undefined,
          data: withGaps(s.points),
          markLine: i === 0 && lines.length ? { silent: true, symbol: "none", label: { show: false }, data: lines } : undefined,
          markArea:
            i === 0 && bands?.length
              ? { silent: true, itemStyle: { color: fade(c.bad, 0.1) }, data: bands.map(([a, b]) => [{ xAxis: a }, { xAxis: b }]) }
              : undefined,
        };
      }),
    };
  }, [series, from, to, marks, bands, levels, step, unit, zoom, decimals, c]);

  // A click anywhere in the plot picks the time under it, not only a
  // click that lands on a data point: the moment wanted is often between
  // samples or where a line has a gap.
  const pick = useRef(onPick);
  useEffect(() => {
    pick.current = onPick;
  }, [onPick]);
  const onReady = (chart: ChartInstance) => {
    chart.getZr().on("click", (e: { offsetX: number; offsetY: number }) => {
      const at: [number, number] = [e.offsetX, e.offsetY];
      if (!pick.current || !chart.containPixel("grid", at)) return;
      const [ms] = chart.convertFromPixel({ gridIndex: 0 }, at) as number[];
      if (Number.isFinite(ms)) pick.current(Math.round(ms));
    });
  };

  return (
    <ReactEChartsCore
      echarts={echarts}
      option={option}
      notMerge
      style={{ height, cursor: onPick ? "crosshair" : undefined }}
      onChartReady={onPick ? onReady : undefined}
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
export function Sparkline({ points, color, height = 36 }: { points: Point[]; color?: string; height?: number }) {
  const c = useThemeColors();
  const line = color ?? c.accent;
  const option = useMemo(
    () => ({
      animation: false,
      grid: { left: 0, right: 0, top: 2, bottom: 2 },
      xAxis: { type: "time", show: false },
      yAxis: { type: "value", show: false, scale: true },
      series: [{ type: "line", showSymbol: false, smooth: 0.3, data: withGaps(points), color: line, lineStyle: { width: 2 }, areaStyle: areaFill(line) }],
    }),
    [points, line],
  );
  return <ReactEChartsCore echarts={echarts} option={option} notMerge style={{ height }} />;
}
