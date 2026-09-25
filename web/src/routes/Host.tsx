import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Activity, Clock, Cpu, HardDrive, MemoryStick, RotateCw, Play, Server, Square } from "lucide-react";

import { api, type HostHealth, type ResourceSummary, type UnitState } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { useCaps, type Pending } from "@/features/trading";
import { Sparkline, TimeSeries, type Point } from "@/ui/charts";
import { Ago, Badge, Button, Card, Freshness, PageHeader, Segmented, Stat, StatusDot, Table, cx, fmtBytes, fmtDuration, type Tone } from "@/ui/kit";

const REFRESH = 10_000;

const WINDOWS: { value: number; label: string }[] = [
  { value: 6, label: "6 小时" },
  { value: 24, label: "24 小时" },
  { value: 24 * 7, label: "7 天" },
  { value: 24 * 30, label: "30 天" },
];

/**
 * The machine and the services on it (docs/UI-V4 §4.5): what runs, with
 * start, stop and restart on the service's own row; the host's load,
 * memory, disks and clock; and each service's memory and CPU over time
 * from the black box. Every number is read from the host through the
 * agent; when the agent cannot say, the page says so rather than showing
 * an empty, healthy-looking box. The trader itself lives on 实盘.
 */
export function Host() {
  const caps = useCaps();
  const writable = caps.data?.writes.available === true;
  const [pending, setPending] = useState<Pending | null>(null);
  const [hours, setHours] = useState(24);

  const units = useQuery({ queryKey: ["ops", "units"], queryFn: api.units, refetchInterval: REFRESH });
  const host = useQuery({ queryKey: ["ops", "host"], queryFn: api.host, refetchInterval: 30_000 });
  const resources = useQuery({ queryKey: ["ops", "resources", hours], queryFn: () => api.resources(hours), refetchInterval: 60_000 });

  return (
    <div className="space-y-5">
      <PageHeader
        title="主机与服务"
        description={`${host.data?.name ? `${host.data.name} · ` : ""}服务启停、主机资源，以及每个服务的内存与 CPU 曲线。`}
        meta={<Freshness at={units.dataUpdatedAt} fetching={units.isFetching} staleAfterS={30} onRefresh={() => void units.refetch()} />}
      />

      {host.isError ? <ErrorState error={host.error} what="主机状态" /> : host.data ? <HostStats h={host.data} /> : <Skeleton tiles={4} rows={0} />}

      <Card
        title="服务"
        icon={<Server className="h-4 w-4" />}
        extra={units.data && <UnitsSummary units={units.data} />}
        bodyClassName="p-0"
      >
        {units.isError ? (
          <div className="p-4">
            <ErrorState error={units.error} what="服务状态" />
          </div>
        ) : !units.data ? (
          <div className="p-4">
            <Skeleton rows={4} />
          </div>
        ) : units.data.length === 0 ? (
          <div className="p-4">
            <Empty title="主机代理没有报告任何服务。" next="在 oq-agent 的配置里列出要管理的 systemd 服务，然后重启 oq-agent。" />
          </div>
        ) : (
          <UnitsTable units={units.data} resources={resources.data} hours={hours} writable={writable} onAct={setPending} />
        )}
        {!writable && caps.data && <p className="border-t border-line px-4 py-2.5 text-xs text-ink-faint">操作按钮未显示：{caps.data.writes.reason}</p>}
      </Card>

      <ResourceHistory q={resources} hours={hours} setHours={setHours} />

      {pending && <ActionDialog {...pending} onClose={() => setPending(null)} />}
    </div>
  );
}

// -- host ---------------------------------------------------------------------

function HostStats({ h }: { h: HostHealth }) {
  const memUsed = h.mem_total && h.mem_available != null ? 1 - h.mem_available / h.mem_total : null;
  return (
    <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <Stat
        label="负载"
        icon={<Activity className="h-3.5 w-3.5" />}
        value={<span className="tabular-nums">{h.load[0]?.toFixed(2) ?? "—"}</span>}
        sub={h.load.length ? `1 / 5 / 15 分钟：${h.load.map((l) => l.toFixed(2)).join(" / ")}` : undefined}
      />
      <Stat
        label="内存"
        icon={<MemoryStick className="h-3.5 w-3.5" />}
        value={memUsed === null ? "—" : <span className="tabular-nums">{(memUsed * 100).toFixed(0)}% 已用</span>}
        sub={memUsed === null ? "主机代理没有报告内存" : <UsageBar used={memUsed} caption={`可用 ${fmtBytes(h.mem_available)} / 共 ${fmtBytes(h.mem_total)}`} />}
      />
      <Stat
        label="已运行"
        icon={<Clock className="h-3.5 w-3.5" />}
        value={h.uptime_s == null ? "—" : fmtDuration(h.uptime_s)}
        sub={h.uptime_s == null ? "主机代理没有报告" : "自主机上次启动"}
      />
      <Stat
        label="时钟"
        icon={<Clock className="h-3.5 w-3.5" />}
        value={h.clock_synced === null ? "未知" : h.clock_synced ? "已同步" : "未同步"}
        tone={h.clock_synced === false ? "bad" : h.clock_synced === null ? "warn" : "good"}
        sub={h.clock_synced === false ? "系统时钟没有与 NTP 同步：时间戳和延迟都不可信" : "与 NTP 同步"}
      />
      {h.disks.map((d) => {
        const used = d.size && d.used != null ? d.used / d.size : null;
        const free = d.size && d.avail != null ? d.avail / d.size : null;
        return (
          <Stat
            key={d.mount}
            label={`磁盘 ${d.mount}`}
            icon={<HardDrive className="h-3.5 w-3.5" />}
            value={free === null ? "—" : <span className="tabular-nums">{(free * 100).toFixed(0)}% 可用</span>}
            tone={free !== null && free < 0.1 ? "bad" : undefined}
            sub={used === null ? "主机代理没有报告大小" : <UsageBar used={used} caption={`可用 ${fmtBytes(d.avail)} / 共 ${fmtBytes(d.size)}`} />}
          />
        );
      })}
    </div>
  );
}

/** How full, as a bar: red once less than a tenth is left, the level alerts fire at. */
function UsageBar({ used, caption }: { used: number; caption: string }) {
  const pct = Math.min(100, Math.max(0, used * 100));
  return (
    <span className="block">
      <span className="mb-1 block h-1.5 overflow-hidden rounded-full bg-surface-raised">
        <span className={cx("block h-full rounded-full", pct >= 90 ? "bg-bad" : "bg-good/70")} style={{ width: `${pct}%` }} />
      </span>
      <span className="tabular-nums">{caption}</span>
    </span>
  );
}

// -- services -----------------------------------------------------------------

function unitTone(u: UnitState): Tone {
  if (u.error) return "warn";
  if (u.ActiveState === "active") return "good";
  if (u.ActiveState === "activating" || u.ActiveState === "deactivating" || u.ActiveState === "reloading") return "warn";
  return "bad";
}

function UnitsSummary({ units }: { units: UnitState[] }) {
  const down = units.filter((u) => u.ActiveState !== "active").length;
  return down ? <Badge tone="bad">{down} 个未运行</Badge> : <Badge tone="good">{units.length} 个全部运行</Badge>;
}

/**
 * `systemctl show` prints the start time as "Thu 2026-09-25 10:00:00 UTC".
 * Only a zone that names an offset unambiguously is converted; an
 * abbreviation like CST (China? US Central?) is shown as written rather
 * than guessed.
 */
function parseSystemdTime(s: string | undefined): number | null {
  const m = s?.match(/(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})(?: (\S+))?/);
  if (!m) return null;
  const zone = m[3];
  const offset = zone === undefined || zone === "UTC" || zone === "GMT" ? "Z" : /^[+-]\d{2}:?\d{2}$/.test(zone) ? zone : null;
  if (offset === null) return null;
  const ms = Date.parse(`${m[1]}T${m[2]}${offset.length === 5 ? `${offset.slice(0, 3)}:${offset.slice(3)}` : offset}`);
  return Number.isFinite(ms) ? ms : null;
}

const VERBS: { verb: "start" | "stop" | "restart"; label: string; note: string; icon: React.ReactNode; variant: "secondary" | "danger" | "primary" }[] = [
  {
    verb: "restart",
    label: "重启",
    note: "停止（进程会撤掉自己的全部挂单）后再启动；策略重新接管持仓并补挂单。",
    icon: <RotateCw className="h-3.5 w-3.5" />,
    variant: "secondary",
  },
  { verb: "stop", label: "停止", note: "停止服务。交易进程会撤掉全部挂单后退出，之后持仓无人管理。", icon: <Square className="h-3.5 w-3.5" />, variant: "danger" },
  { verb: "start", label: "启动", note: "启动服务。", icon: <Play className="h-3.5 w-3.5" />, variant: "primary" },
];

const mib = (b: number | null | undefined) => (b == null ? "—" : `${(b / 1_048_576).toFixed(1)} MiB`);
const pct = (v: number | null | undefined, digits = 3) => (v == null ? "—" : `${v.toFixed(digits)}%`);

/** The newest sample of a service's curve that carries a value. */
function latest(r: ResourceSummary | undefined, key: "mem" | "cpu"): number | null {
  if (!r) return null;
  for (let i = r.curve.length - 1; i >= 0; i--) {
    const v = r.curve[i][key];
    if (v != null) return v;
  }
  return null;
}

function UnitsTable({
  units,
  resources,
  hours,
  writable,
  onAct,
}: {
  units: UnitState[];
  resources: ResourceSummary[] | undefined;
  hours: number;
  writable: boolean;
  onAct: (p: Pending) => void;
}) {
  const span = WINDOWS.find((w) => w.value === hours)?.label ?? `${hours} 小时`;
  return (
    <Table head={["服务", "状态", "启动于", "重启次数", "内存", "CPU（单核）", ""]}>
      {units.map((u) => {
        const up = u.ActiveState === "active";
        const tone = unitTone(u);
        const started = u.started_ms ?? parseSystemdTime(u.ExecMainStartTimestamp);
        const r = resources?.find((x) => x.unit === u.unit);
        const mem = latest(r, "mem");
        const cpu = latest(r, "cpu");
        return (
          <tr key={u.unit} className="align-top">
            <td className="font-mono text-xs text-ink">{u.unit}</td>
            <td>
              <span className="inline-flex items-center gap-2">
                <StatusDot tone={tone} pulse={tone === "bad"} />
                <span className={cx(tone === "bad" ? "text-bad" : tone === "warn" ? "text-warn" : "text-ink")}>
                  {u.error ? "无法获取" : `${u.ActiveState ?? "—"} / ${u.SubState ?? "—"}`}
                </span>
              </span>
              {u.error && <div className="mt-0.5 text-xs text-ink-faint">{u.error}</div>}
            </td>
            <td className="whitespace-nowrap text-xs text-ink-muted">{started !== null ? <Ago ms={started} /> : u.ExecMainStartTimestamp || "—"}</td>
            <td className={cx("tabular-nums", Number(u.NRestarts ?? 0) > 0 ? "text-warn" : "text-ink-muted")}>{u.NRestarts ?? "—"}</td>
            <td className="whitespace-nowrap tabular-nums">
              {mem === null ? (
                <span className="text-ink-faint">—</span>
              ) : (
                <>
                  <div className="text-ink">{mib(mem)}</div>
                  {r?.memory && (
                    <div className="text-xs text-ink-faint">
                      {span}峰值 {mib(r.memory.max)}
                    </div>
                  )}
                </>
              )}
            </td>
            <td className="whitespace-nowrap tabular-nums">
              {cpu === null ? (
                <span className="text-ink-faint">—</span>
              ) : (
                <>
                  <div className="text-ink">{pct(cpu, 2)}</div>
                  {r?.cpu_percent && <div className="text-xs text-ink-faint">p95 {pct(r.cpu_percent.p95, 2)}</div>}
                </>
              )}
            </td>
            <td className="text-right">
              {writable && u.manageable && (
                <div className="inline-flex gap-1.5">
                  {VERBS.filter((v) => (up ? v.verb !== "start" : v.verb === "start")).map((v) => (
                    <Button
                      key={v.verb}
                      size="sm"
                      variant={v.variant}
                      icon={v.icon}
                      onClick={() =>
                        onAct({
                          title: `${v.label} ${u.unit}`,
                          consequence: v.note,
                          action: { action: "unit", unit: u.unit, verb: v.verb },
                          highRisk: true,
                        })
                      }
                    >
                      {v.label}
                    </Button>
                  ))}
                </div>
              )}
            </td>
          </tr>
        );
      })}
    </Table>
  );
}

// -- resource history ---------------------------------------------------------------

/**
 * The black box (host agent, every 30 s, kept 30 days): each service's
 * memory and CPU with its extremes over a window. systemd forgets both at
 * every restart; these samples do not.
 */
function ResourceHistory({
  q,
  hours,
  setHours,
}: {
  q: { data: ResourceSummary[] | undefined; isLoading: boolean; isError: boolean; error: unknown };
  hours: number;
  setHours: (h: number) => void;
}) {
  const to = Date.now();
  const from = to - hours * 3_600_000;
  const rows = q.data ?? [];
  const recorded = rows.filter((r) => r.samples > 0);
  return (
    <Card
      title="资源曲线"
      icon={<Cpu className="h-4 w-4" />}
      extra={
        <>
          <span className="hidden sm:inline">主机代理每 30 秒记录一次，保留 90 天</span>
          <Segmented value={hours} options={WINDOWS} onChange={setHours} />
        </>
      }
    >
      {q.isLoading ? (
        <Skeleton rows={5} />
      ) : q.isError ? (
        <ErrorState error={q.error} what="资源记录" />
      ) : rows.length === 0 ? (
        <Empty title="还没有资源记录。" next="主机代理启动后每 30 秒记录一次各服务的内存和 CPU；稍等片刻再看。" />
      ) : (
        <div className="space-y-5">
          {recorded.length > 0 && (
            <div className="grid gap-4 lg:grid-cols-2">
              <div>
                <div className="mb-1 text-xs text-ink-muted">内存（MiB）</div>
                <TimeSeries
                  height={180}
                  from={from}
                  to={to}
                  decimals={1}
                  series={recorded.map((r) => ({
                    name: r.unit.replace(".service", ""),
                    points: r.curve.map((p) => [p.at, p.mem == null ? null : p.mem / 1_048_576] as Point),
                  }))}
                />
              </div>
              <div>
                <div className="mb-1 text-xs text-ink-muted">CPU（单核 %）</div>
                <TimeSeries
                  height={180}
                  from={from}
                  to={to}
                  decimals={2}
                  unit="%"
                  series={recorded.map((r) => ({
                    name: r.unit.replace(".service", ""),
                    points: r.curve.map((p) => [p.at, p.cpu] as Point),
                  }))}
                />
              </div>
            </div>
          )}
          <Table head={["服务", "内存 最小 / 平均 / p95 / 最大", "CPU（单核）平均 / p95 / 最大", "重启", "内存走势"]}>
            {rows.map((r) => (
              <ResourceRow key={r.unit} r={r} hours={hours} />
            ))}
          </Table>
        </div>
      )}
    </Card>
  );
}

function ResourceRow({ r, hours }: { r: ResourceSummary; hours: number }) {
  const covered = r.first_ms ? (Date.now() - r.first_ms) / 3.6e6 : 0;
  const points: Point[] = r.curve.map((p) => [p.at, p.mem]);
  return (
    <tr className="align-top">
      <td className="font-mono text-xs text-ink">
        {r.unit}
        {r.samples === 0 ? (
          <div className="mt-0.5 font-sans text-warn">还没有记录</div>
        ) : covered < hours * 0.9 ? (
          <div className="mt-0.5 font-sans text-ink-faint">记录只覆盖最近 {covered.toFixed(1)} 小时</div>
        ) : null}
      </td>
      <td className="whitespace-nowrap text-xs tabular-nums text-ink-muted">
        {r.memory ? `${mib(r.memory.min)} / ${mib(r.memory.avg)} / ${mib(r.memory.p95)} / ${mib(r.memory.max)}` : "—"}
      </td>
      <td className="whitespace-nowrap text-xs tabular-nums text-ink-muted">
        {r.cpu_percent ? `${pct(r.cpu_percent.avg)} / ${pct(r.cpu_percent.p95)} / ${pct(r.cpu_percent.max)}` : "—"}
      </td>
      <td className={cx("whitespace-nowrap text-xs", r.process_changes > 0 ? "text-warn" : "text-ink-muted")}>
        {r.process_changes} 次{r.samples_down > 0 ? ` · 停止 ${Math.round((r.samples_down * 30) / 60)} 分钟` : ""}
      </td>
      <td className="w-44">{points.filter((p) => p[1] != null).length >= 2 ? <Sparkline points={points} height={28} /> : <span className="text-xs text-ink-faint">—</span>}</td>
    </tr>
  );
}
