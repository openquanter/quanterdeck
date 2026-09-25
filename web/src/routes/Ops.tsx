import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import {
  api,
  ApiError,
  type HostHealth,
  type OpsAction,
  type TraderStatus,
  type UnitState,
} from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";

const REFRESH = 10_000;

type Pending = { title: string; consequence: string; action: OpsAction; highRisk: boolean };

/**
 * The host at a glance: what is raised, what the trader says about
 * itself, which units run, and the machine under them. Every number here
 * is read from the host through the agent; when the agent cannot say, the
 * page says "无法获取" rather than showing an empty, healthy-looking box.
 */
export function Ops() {
  const caps = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const writable = caps.data?.writes.available === true;
  const [pending, setPending] = useState<Pending | null>(null);

  const alerts = useQuery({ queryKey: ["ops", "alerts"], queryFn: api.alerts, refetchInterval: REFRESH });
  const status = useQuery({
    queryKey: ["ops", "status"],
    queryFn: api.traderStatus,
    refetchInterval: REFRESH,
    retry: false,
  });
  const units = useQuery({ queryKey: ["ops", "units"], queryFn: api.units, refetchInterval: REFRESH });
  const host = useQuery({ queryKey: ["ops", "host"], queryFn: api.host, refetchInterval: 30_000 });

  return (
    <div className="space-y-6">
      <h1 className="text-lg text-ink">运维总览</h1>

      <section>
        <h2 className="mb-2 text-sm text-ink-muted">告警</h2>
        {alerts.isError ? (
          <Unknown what="告警" error={alerts.error} />
        ) : alerts.data && alerts.data.length === 0 ? (
          <p className="text-sm text-good">无</p>
        ) : (
          <ul className="space-y-1">
            {alerts.data?.map((a) => (
              <li key={a.key} className="rounded border border-bad/50 bg-bad/10 px-3 py-2 text-sm text-ink">
                {a.message}
                <span className="ml-2 text-xs text-ink-muted">自 {time(a.since_ms)}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section>
        <div className="mb-2 flex items-center gap-3">
          <h2 className="text-sm text-ink-muted">交易进程</h2>
          {writable && status.data && (
            <div className="ml-auto flex gap-2">
              {!status.data.halted && (
                <Button
                  danger
                  label="停机"
                  onClick={() =>
                    setPending({
                      title: "停机（halt）",
                      consequence:
                        "停止开新仓，撤掉开仓挂单，保留止盈等平仓单，进程继续看着持仓。出事时用这个。",
                      action: { action: "halt" },
                      highRisk: false,
                    })
                  }
                />
              )}
              {status.data.halted && status.data.resume_allowed && (
                <Button
                  label="解除停机"
                  onClick={() =>
                    setPending({
                      title: "解除停机（resume）",
                      consequence:
                        "清除停机状态，策略恢复下单。进程会先确认最近一次持仓核对一致、日志可写，否则拒绝。",
                      action: { action: "resume" },
                      highRisk: true,
                    })
                  }
                />
              )}
              <Button
                label="退出进程"
                onClick={() =>
                  setPending({
                    title: "退出进程（shutdown）",
                    consequence:
                      "撤掉全部挂单（包括止盈），然后退出且不会自动重启：持仓将无人管理、没有止盈保护。只用于计划内维护。",
                    action: { action: "shutdown" },
                    highRisk: true,
                  })
                }
              />
            </div>
          )}
        </div>
        {status.isError ? (
          <Unknown what="交易进程状态" error={status.error} />
        ) : status.data ? (
          <TraderPanel s={status.data} />
        ) : (
          <p className="text-sm text-ink-muted">读取中…</p>
        )}
      </section>

      <section>
        <h2 className="mb-2 text-sm text-ink-muted">服务</h2>
        {units.isError ? (
          <Unknown what="服务状态" error={units.error} />
        ) : (
          <UnitsTable units={units.data ?? []} writable={writable} onAct={setPending} />
        )}
      </section>

      <section>
        <h2 className="mb-2 text-sm text-ink-muted">主机</h2>
        {host.isError ? <Unknown what="主机状态" error={host.error} /> : host.data && <HostTiles h={host.data} />}
      </section>

      {!writable && caps.data && (
        <p className="text-xs text-ink-muted">操作按钮未显示：{caps.data.writes.reason}</p>
      )}

      {pending && <ActionDialog {...pending} onClose={() => setPending(null)} />}
    </div>
  );
}

function TraderPanel({ s }: { s: TraderStatus }) {
  const lastTickAge = s.last_tick ? (s.now_ns - s.last_tick.local_ns) / 1e9 : null;
  return (
    <div className="space-y-3">
      <div className="grid gap-3 sm:grid-cols-4">
        <Tile
          label="状态"
          value={s.halted ? "已停机" : "交易中"}
          tone={s.halted ? "bad" : "good"}
          sub={s.halted ? (s.halt_reason ?? undefined) : `${s.strategy} · ${s.symbol} · ${s.deployment}`}
        />
        <Tile
          label="持仓"
          value={s.positions.length === 0 ? "无" : s.positions.map((p) => `${p.side} ${p.amount}`).join("  ")}
        />
        <Tile label="挂单" value={String(s.resting)} />
        <Tile
          label="最近行情"
          value={lastTickAge === null ? "尚无" : `${lastTickAge.toFixed(0)} 秒前`}
          tone={lastTickAge !== null && lastTickAge > 120 ? "bad" : undefined}
          sub={s.last_tick ? `last ${s.last_tick.last}` : undefined}
        />
      </div>
      <div className="grid gap-3 sm:grid-cols-4">
        <Tile
          label="持仓核对"
          value={s.reconcile.agreed === null ? "尚未核对" : s.reconcile.agreed ? "一致" : "不一致"}
          tone={s.reconcile.agreed === false ? "bad" : s.reconcile.agreed ? "good" : undefined}
          sub={`累计不一致 ${s.reconcile.mismatches}，读取失败 ${s.reconcile.unread}`}
        />
        <Tile
          label="行情"
          value={`读不出 ${s.feed.unreadable}`}
          tone={s.feed.unreadable > 0 ? "bad" : undefined}
          sub={`深度 ${s.feed.depth} · 成交 ${s.feed.trades} · 重同步 ${s.feed.resyncs}`}
        />
        <Tile
          label="日志"
          value={s.journal_lost ? "无法写入" : "正常"}
          tone={s.journal_lost ? "bad" : "good"}
          sub={s.journal_lost ?? undefined}
        />
        <Tile label="进程" value={`pid ${s.pid}`} sub={`tick ${s.ticks} · 前缀 ${s.prefix}`} />
      </div>
      {Object.keys(s.waiting_on).length > 0 && (
        <div className="rounded border border-line bg-surface p-3 text-xs text-ink-muted">
          <span className="mr-2 text-ink">策略在等：</span>
          {Object.entries(s.waiting_on).map(([k, v]) => (
            <span key={k} className="mr-3 font-mono">
              {k} {v}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function UnitsTable({
  units,
  writable,
  onAct,
}: {
  units: UnitState[];
  writable: boolean;
  onAct: (p: Pending) => void;
}) {
  const verbs: { verb: "start" | "stop" | "restart"; label: string; note: string }[] = [
    { verb: "restart", label: "重启", note: "停止（进程会撤掉自己的全部挂单）后再启动；策略重新接管持仓并补挂单。" },
    { verb: "stop", label: "停止", note: "停止服务。交易进程会撤掉全部挂单后退出，之后持仓无人管理。" },
    { verb: "start", label: "启动", note: "启动服务。" },
  ];
  return (
    <table className="w-full text-sm">
      <thead className="text-left text-xs text-ink-muted">
        <tr>
          <th className="py-1">服务</th>
          <th>状态</th>
          <th>启动于</th>
          <th>重启次数</th>
          <th />
        </tr>
      </thead>
      <tbody>
        {units.map((u) => {
          const up = u.ActiveState === "active";
          return (
            <tr key={u.unit} className="border-t border-line">
              <td className="py-1.5 font-mono text-xs">{u.unit}</td>
              <td className={up ? "text-good" : "text-bad"}>
                {u.error ? `无法获取：${u.error}` : `${u.ActiveState} / ${u.SubState}`}
              </td>
              <td className="text-xs text-ink-muted">{u.ExecMainStartTimestamp || "—"}</td>
              <td className="text-xs text-ink-muted">{u.NRestarts ?? "—"}</td>
              <td className="text-right">
                {writable && u.manageable &&
                  verbs
                    .filter((v) => (up ? v.verb !== "start" : v.verb === "start"))
                    .map((v) => (
                      <button
                        key={v.verb}
                        className="ml-2 text-xs text-accent hover:underline"
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
                      </button>
                    ))}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function HostTiles({ h }: { h: HostHealth }) {
  const memUsed =
    h.mem_total && h.mem_available ? 1 - h.mem_available / h.mem_total : null;
  return (
    <div className="grid gap-3 sm:grid-cols-4">
      <Tile label="负载" value={h.load.map((l) => l.toFixed(2)).join(" / ")} />
      <Tile label="内存" value={memUsed === null ? "—" : `${(memUsed * 100).toFixed(0)}% 已用`} />
      {h.disks.map((d) => {
        const free = d.size && d.avail !== null ? d.avail / d.size : null;
        return (
          <Tile
            key={d.mount}
            label={`磁盘 ${d.mount}`}
            value={free === null ? "—" : `${(free * 100).toFixed(0)}% 可用`}
            tone={free !== null && free < 0.1 ? "bad" : undefined}
            sub={d.avail !== null ? `${gib(d.avail)} / ${gib(d.size)}` : undefined}
          />
        );
      })}
      <Tile
        label="时钟"
        value={h.clock_synced === null ? "未知" : h.clock_synced ? "已同步" : "未同步"}
        tone={h.clock_synced === false ? "bad" : undefined}
      />
    </div>
  );
}

export function Tile({
  label,
  value,
  sub,
  tone,
}: {
  label: string;
  value: string;
  sub?: string;
  tone?: "good" | "bad";
}) {
  const color = tone === "bad" ? "text-bad" : tone === "good" ? "text-good" : "text-ink";
  return (
    <div className="rounded border border-line bg-surface p-3">
      <div className="text-xs text-ink-muted">{label}</div>
      <div className={`mt-1 text-base ${color}`}>{value}</div>
      {sub && <div className="mt-1 break-words text-xs text-ink-muted">{sub}</div>}
    </div>
  );
}

export function Unknown({ what, error }: { what: string; error: unknown }) {
  const detail = error instanceof ApiError ? error.detail : String(error);
  return (
    <p className="rounded border border-warn/50 bg-warn/10 px-3 py-2 text-sm text-ink">
      无法获取{what}：{detail}
    </p>
  );
}

export function Button({ label, onClick, danger }: { label: string; onClick: () => void; danger?: boolean }) {
  return (
    <button
      className={[
        "rounded border px-3 py-1 text-xs",
        danger ? "border-bad text-bad hover:bg-bad/10" : "border-line text-ink hover:bg-surface-raised",
      ].join(" ")}
      onClick={onClick}
    >
      {label}
    </button>
  );
}

export function time(ms: number) {
  return new Date(ms).toLocaleString("zh-CN", { hour12: false });
}

function gib(bytes: number | null) {
  return bytes === null ? "—" : `${(bytes / 2 ** 30).toFixed(0)} GiB`;
}
