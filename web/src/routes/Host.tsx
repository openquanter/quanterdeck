import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Activity, Clock, Cpu, HardDrive, MemoryStick, RotateCw, Play, Server, Square } from "lucide-react";

import { api, type HostHealth, type ResourceSummary, type UnitState } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { useCaps, type Pending } from "@/features/trading";
import { tr } from "@/i18n";
import { Sparkline, TimeSeries, type Point } from "@/ui/charts";
import { Ago, Badge, Button, Card, Freshness, PageHeader, Segmented, Stat, StatusDot, Table, cx, fmtBytes, fmtDuration, type Tone } from "@/ui/kit";

const REFRESH = 10_000;

const windows = (): { value: number; label: string }[] => [
  { value: 6, label: tr("6 小时", "6 h") },
  { value: 24, label: tr("24 小时", "24 h") },
  { value: 24 * 7, label: tr("7 天", "7 d") },
  { value: 24 * 30, label: tr("30 天", "30 d") },
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

  const hostPrefix = host.data?.name ? `${host.data.name} · ` : "";

  return (
    <div className="space-y-5">
      <PageHeader
        title={tr("主机与服务", "Host and services")}
        description={tr(
          `${hostPrefix}服务启停、主机资源，以及每个服务的内存与 CPU 曲线。`,
          `${hostPrefix}Service control, host resources, and each service's memory and CPU over time.`,
        )}
        meta={<Freshness at={units.dataUpdatedAt} fetching={units.isFetching} staleAfterS={30} onRefresh={() => void units.refetch()} />}
      />

      {host.isError ? <ErrorState error={host.error} what={tr("主机状态", "host status")} /> : host.data ? <HostStats h={host.data} /> : <Skeleton tiles={4} rows={0} />}

      <Card
        title={tr("服务", "Services")}
        icon={<Server className="h-4 w-4" />}
        extra={units.data && <UnitsSummary units={units.data} />}
        bodyClassName="p-0"
      >
        {units.isError ? (
          <div className="p-4">
            <ErrorState error={units.error} what={tr("服务状态", "service status")} />
          </div>
        ) : !units.data ? (
          <div className="p-4">
            <Skeleton rows={4} />
          </div>
        ) : units.data.length === 0 ? (
          <div className="p-4">
            <Empty
              title={tr("主机代理没有报告任何服务。", "The host agent reports no services.")}
              next={tr("在 oq-agent 的配置里列出要管理的 systemd 服务，然后重启 oq-agent。", "List the systemd services to manage in the oq-agent config, then restart oq-agent.")}
            />
          </div>
        ) : (
          <UnitsTable units={units.data} resources={resources.data} hours={hours} writable={writable} onAct={setPending} />
        )}
        {!writable && caps.data && (
          <p className="border-t border-line px-4 py-2.5 text-xs text-ink-faint">
            {tr("操作按钮未显示：", "Action buttons hidden: ")}
            {caps.data.writes.reason}
          </p>
        )}
      </Card>

      <ResourceHistory q={resources} hours={hours} setHours={setHours} />

      {pending && <ActionDialog {...pending} onClose={() => setPending(null)} />}
    </div>
  );
}

// -- host ---------------------------------------------------------------------

function HostStats({ h }: { h: HostHealth }) {
  const memUsed = h.mem_total && h.mem_available != null ? 1 - h.mem_available / h.mem_total : null;
  const loads = h.load.map((l) => l.toFixed(2)).join(" / ");
  return (
    <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <Stat
        label={tr("负载", "Load")}
        icon={<Activity className="h-3.5 w-3.5" />}
        value={<span className="tabular-nums">{h.load[0]?.toFixed(2) ?? "—"}</span>}
        sub={h.load.length ? tr(`1 / 5 / 15 分钟：${loads}`, `1 / 5 / 15 min: ${loads}`) : undefined}
      />
      <Stat
        label={tr("内存", "Memory")}
        icon={<MemoryStick className="h-3.5 w-3.5" />}
        value={memUsed === null ? "—" : <span className="tabular-nums">{tr(`${(memUsed * 100).toFixed(0)}% 已用`, `${(memUsed * 100).toFixed(0)}% used`)}</span>}
        sub={
          memUsed === null ? (
            tr("主机代理没有报告内存", "The host agent did not report memory")
          ) : (
            <UsageBar
              used={memUsed}
              caption={tr(`可用 ${fmtBytes(h.mem_available)} / 共 ${fmtBytes(h.mem_total)}`, `${fmtBytes(h.mem_available)} free of ${fmtBytes(h.mem_total)}`)}
            />
          )
        }
      />
      <Stat
        label={tr("已运行", "Uptime")}
        icon={<Clock className="h-3.5 w-3.5" />}
        value={h.uptime_s == null ? "—" : fmtDuration(h.uptime_s)}
        sub={h.uptime_s == null ? tr("主机代理没有报告", "Not reported by the host agent") : tr("自主机上次启动", "Since the host last booted")}
      />
      <Stat
        label={tr("时钟", "Clock")}
        icon={<Clock className="h-3.5 w-3.5" />}
        value={h.clock_synced === null ? tr("未知", "Unknown") : h.clock_synced ? tr("已同步", "Synced") : tr("未同步", "Not synced")}
        tone={h.clock_synced === false ? "bad" : h.clock_synced === null ? "warn" : "good"}
        sub={
          h.clock_synced === false
            ? tr("系统时钟没有与 NTP 同步：时间戳和延迟都不可信", "The system clock is not synced to NTP: timestamps and latencies cannot be trusted")
            : tr("与 NTP 同步", "Synced to NTP")
        }
      />
      {h.disks.map((d) => {
        const used = d.size && d.used != null ? d.used / d.size : null;
        const free = d.size && d.avail != null ? d.avail / d.size : null;
        return (
          <Stat
            key={d.mount}
            label={tr(`磁盘 ${d.mount}`, `Disk ${d.mount}`)}
            icon={<HardDrive className="h-3.5 w-3.5" />}
            value={free === null ? "—" : <span className="tabular-nums">{tr(`${(free * 100).toFixed(0)}% 可用`, `${(free * 100).toFixed(0)}% free`)}</span>}
            tone={free !== null && free < 0.1 ? "bad" : undefined}
            sub={
              used === null ? (
                tr("主机代理没有报告大小", "The host agent did not report the size")
              ) : (
                <UsageBar used={used} caption={tr(`可用 ${fmtBytes(d.avail)} / 共 ${fmtBytes(d.size)}`, `${fmtBytes(d.avail)} free of ${fmtBytes(d.size)}`)} />
              )
            }
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
  return down ? (
    <Badge tone="bad">{tr(`${down} 个未运行`, `${down} not running`)}</Badge>
  ) : (
    <Badge tone="good">{tr(`${units.length} 个全部运行`, `All ${units.length} running`)}</Badge>
  );
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

const verbs = (): { verb: "start" | "stop" | "restart"; label: string; note: string; icon: React.ReactNode; variant: "secondary" | "danger" | "primary" }[] => [
  {
    verb: "restart",
    label: tr("重启", "Restart"),
    note: tr(
      "停止（进程会撤掉自己的全部挂单）后再启动；策略重新接管持仓并补挂单。",
      "Stops (the process cancels all its open orders) and starts again; the strategy takes its positions back over and re-places its orders.",
    ),
    icon: <RotateCw className="h-3.5 w-3.5" />,
    variant: "secondary",
  },
  {
    verb: "stop",
    label: tr("停止", "Stop"),
    note: tr(
      "停止服务。交易进程会撤掉全部挂单后退出，之后持仓无人管理。",
      "Stops the service. The trading process cancels all open orders and exits; its positions are then unmanaged.",
    ),
    icon: <Square className="h-3.5 w-3.5" />,
    variant: "danger",
  },
  { verb: "start", label: tr("启动", "Start"), note: tr("启动服务。", "Starts the service."), icon: <Play className="h-3.5 w-3.5" />, variant: "primary" },
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
  const span = windows().find((w) => w.value === hours)?.label ?? tr(`${hours} 小时`, `${hours} h`);
  return (
    <Table
      head={[tr("服务", "Service"), tr("状态", "State"), tr("启动于", "Started"), tr("重启次数", "Restarts"), tr("内存", "Memory"), tr("CPU（单核）", "CPU (one core)"), ""]}
    >
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
                  {u.error ? tr("无法获取", "Unavailable") : `${u.ActiveState ?? "—"} / ${u.SubState ?? "—"}`}
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
                      {tr(`${span}峰值 ${mib(r.memory.max)}`, `${span} peak ${mib(r.memory.max)}`)}
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
                  {verbs().filter((v) => (up ? v.verb !== "start" : v.verb === "start")).map((v) => (
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
 * The black box (host agent, every 30 s, kept 90 days): each service's
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
      title={tr("资源曲线", "Resource history")}
      icon={<Cpu className="h-4 w-4" />}
      extra={
        <>
          <span className="hidden sm:inline">{tr("主机代理每 30 秒记录一次，保留 90 天", "Recorded by the host agent every 30 s, kept 90 days")}</span>
          <Segmented value={hours} options={windows()} onChange={setHours} />
        </>
      }
    >
      {q.isLoading ? (
        <Skeleton rows={5} />
      ) : q.isError ? (
        <ErrorState error={q.error} what={tr("资源记录", "resource records")} />
      ) : rows.length === 0 ? (
        <Empty
          title={tr("还没有资源记录。", "No resource records yet.")}
          next={tr(
            "主机代理启动后每 30 秒记录一次各服务的内存和 CPU；稍等片刻再看。",
            "Once started, the host agent records each service's memory and CPU every 30 s; check back shortly.",
          )}
        />
      ) : (
        <div className="space-y-5">
          {recorded.length > 0 && (
            <div className="grid gap-4 lg:grid-cols-2">
              <div>
                <div className="mb-1 text-xs text-ink-muted">{tr("内存（MiB）", "Memory (MiB)")}</div>
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
                <div className="mb-1 text-xs text-ink-muted">{tr("CPU（单核 %）", "CPU (% of one core)")}</div>
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
          <Table
            head={[
              tr("服务", "Service"),
              tr("内存 最小 / 平均 / p95 / 最大", "Memory min / avg / p95 / max"),
              tr("CPU（单核）平均 / p95 / 最大", "CPU (one core) avg / p95 / max"),
              tr("重启", "Restarts"),
              tr("内存走势", "Memory trend"),
            ]}
          >
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
  const downMin = Math.round((r.samples_down * 30) / 60);
  return (
    <tr className="align-top">
      <td className="font-mono text-xs text-ink">
        {r.unit}
        {r.samples === 0 ? (
          <div className="mt-0.5 font-sans text-warn">{tr("还没有记录", "Not recorded yet")}</div>
        ) : covered < hours * 0.9 ? (
          <div className="mt-0.5 font-sans text-ink-faint">{tr(`记录只覆盖最近 ${covered.toFixed(1)} 小时`, `Records cover only the last ${covered.toFixed(1)} h`)}</div>
        ) : null}
      </td>
      <td className="whitespace-nowrap text-xs tabular-nums text-ink-muted">
        {r.memory ? `${mib(r.memory.min)} / ${mib(r.memory.avg)} / ${mib(r.memory.p95)} / ${mib(r.memory.max)}` : "—"}
      </td>
      <td className="whitespace-nowrap text-xs tabular-nums text-ink-muted">
        {r.cpu_percent ? `${pct(r.cpu_percent.avg)} / ${pct(r.cpu_percent.p95)} / ${pct(r.cpu_percent.max)}` : "—"}
      </td>
      <td className={cx("whitespace-nowrap text-xs", r.process_changes > 0 ? "text-warn" : "text-ink-muted")}>
        {tr(`${r.process_changes} 次`, `${r.process_changes}`)}
        {r.samples_down > 0 ? tr(` · 停止 ${downMin} 分钟`, ` · down ${downMin} min`) : ""}
      </td>
      <td className="w-44">{points.filter((p) => p[1] != null).length >= 2 ? <Sparkline points={points} height={28} /> : <span className="text-xs text-ink-faint">—</span>}</td>
    </tr>
  );
}
