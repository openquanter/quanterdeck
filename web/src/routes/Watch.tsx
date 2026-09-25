import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { api, type OpsAction } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Empty, ErrorState, Skeleton, Term } from "@/components/States";

import { Tile } from "./Ops";

const RULES: [string, string][] = [
  ["halted", "交易进程停机"],
  ["reconcile", "进程内持仓核对与交易所不一致"],
  ["journal", "交易日志写不进去（进程已停止开新单）"],
  ["feed", "行情出现读不出的消息"],
  ["control", "交易进程在运行，控制口却无应答"],
  ["unit:<服务>", "受管服务没有在运行（操作者主动停机时不报）"],
  ["disk:<挂载点>", "剩余空间不足 10%"],
  ["clock", "系统时钟未与 NTP 同步"],
];

/**
 * Alerts (blueprint P2 9): what is raised now, what was raised and
 * cleared, the rules, the channel with a test send, and silencing one
 * alert for a while. Edges, not levels: a condition is announced when it
 * starts and when it clears.
 */
export function Alerts() {
  const caps = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const writable = caps.data?.writes.available === true;
  const q = useQuery({ queryKey: ["ops", "alerts", "view"], queryFn: api.alertsView, refetchInterval: 10_000 });
  const [pending, setPending] = useState<{ title: string; consequence: string; action: OpsAction } | null>(null);

  return (
    <div className="space-y-6">
      <div className="flex items-center">
        <h1 className="text-lg text-ink">告警</h1>
        {writable && (
          <button
            className="ml-auto rounded border border-line px-3 py-1 text-xs text-ink hover:bg-surface-raised"
            onClick={() =>
              setPending({
                title: "发送测试消息",
                consequence: "向 Discord「alerts」频道发一条测试消息，确认渠道通着。",
                action: { action: "alert_test" },
              })
            }
          >
            测试告警渠道…
          </button>
        )}
      </div>
      {q.isLoading ? (
        <Skeleton rows={6} />
      ) : q.isError ? (
        <ErrorState error={q.error} what="告警" />
      ) : (
        <>
          <section>
            <h2 className="mb-2 text-sm text-ink-muted">当前</h2>
            {q.data!.active.length === 0 ? (
              <p className="text-sm text-good">没有告警</p>
            ) : (
              <ul className="space-y-1">
                {q.data!.active.map((a) => (
                  <li key={a.key} className="flex items-center gap-3 rounded border border-bad/50 bg-bad/10 px-3 py-2 text-sm">
                    <span className="text-ink">{a.message}</span>
                    <span className="text-xs text-ink-muted">自 {new Date(a.since_ms).toLocaleString("zh-CN", { hour12: false })}</span>
                    {"silenced_until_ms" in a && (a as { silenced_until_ms?: number | null }).silenced_until_ms ? (
                      <span className="ml-auto text-xs text-warn">
                        静默至 {new Date((a as { silenced_until_ms: number }).silenced_until_ms).toLocaleTimeString("zh-CN", { hour12: false })}
                      </span>
                    ) : (
                      writable && (
                        <button
                          className="ml-auto text-xs text-accent hover:underline"
                          onClick={() =>
                            setPending({
                              title: `静默「${a.message}」1 小时`,
                              consequence: "1 小时内这条告警的触发与恢复都不推送；页面上照常显示。",
                              action: { action: "alert_silence", key: a.key, minutes: 60 },
                            })
                          }
                        >
                          静默 1 小时…
                        </button>
                      )
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>
          <section>
            <h2 className="mb-2 text-sm text-ink-muted">最近的触发与恢复</h2>
            {q.data!.history.length === 0 ? (
              <p className="text-xs text-ink-muted">agent 启动以来还没有触发过。</p>
            ) : (
              <ul className="max-h-80 space-y-0.5 overflow-auto text-xs">
                {q.data!.history.map((h, k) => (
                  <li key={k} className="text-ink-muted">
                    <span className="font-mono">{new Date(h.at_ms).toLocaleString("zh-CN", { hour12: false })}</span>{" "}
                    <span className={h.raised ? "text-bad" : "text-good"}>{h.raised ? "触发" : "恢复"}</span> {h.message}
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      )}
      <section>
        <h2 className="mb-2 text-sm text-ink-muted">规则（主机代理每 30 秒检查一次）</h2>
        <table className="text-sm">
          <tbody>
            {RULES.map(([k, v]) => (
              <tr key={k} className="border-t border-line">
                <td className="py-1 pr-4 font-mono text-xs text-ink-muted">{k}</td>
                <td className="py-1">{v}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="mt-2 text-xs text-ink-muted">
          渠道：Discord「alerts」频道（与 1.x 同一个机器人）。每一条操作审计也会同步发到这个频道。
        </p>
      </section>
      {pending && <ActionDialog {...pending} highRisk={false} onClose={() => setPending(null)} />}
    </div>
  );
}

/**
 * Which venue account each process is using (blueprint §7 exchanges):
 * the key's fingerprint, never the key. The console holds no key and
 * cannot ask the venue about permissions; it can show whether the trader
 * and its watcher are looking at the same account, which is the mistake
 * that has actually happened.
 */
export function Accounts() {
  const q = useQuery({ queryKey: ["ops", "accounts"], queryFn: api.accounts, refetchInterval: 60_000 });
  if (q.isLoading) return <Skeleton rows={4} />;
  if (q.isError) return <ErrorState error={q.error} what="账户" />;
  const entries = Object.entries(q.data!.processes);
  return (
    <div className="space-y-4">
      <h1 className="text-lg text-ink">交易所账户</h1>
      {entries.length === 0 ? (
        <Empty title="日志里还没有账户指纹。" next="交易进程和 oq-recon 启动时把所用密钥的指纹写在日志第一行。" />
      ) : (
        <>
          <p className={`text-sm ${q.data!.same_account ? "text-good" : "text-bad"}`}>
            {q.data!.same_account ? "所有进程用的是同一个账户。" : "进程用的不是同一个账户：对账进程看的可能不是交易进程在交易的账户。"}
          </p>
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-ink-muted">
              <tr>
                <th className="py-1 font-normal">进程</th>
                <th className="font-normal">密钥指纹</th>
                <th className="font-normal">来源</th>
              </tr>
            </thead>
            <tbody>
              {entries.map(([p, v]) => (
                <tr key={p} className="border-t border-line">
                  <td className="py-1.5">{p}</td>
                  <td className="font-mono">{v.fingerprint ?? "未写"}</td>
                  <td className="font-mono text-xs text-ink-muted">{v.log}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
      <p className="text-xs text-ink-muted">
        密钥本身只在交易进程的 systemd 凭据里，控制台和主机代理都读不到。API 权限（是否允许提币）只能在交易所后台或主网 API
        查询，测试网不提供这个接口；上主网前请在交易所后台确认密钥没有提币权限。
      </p>
    </div>
  );
}

/**
 * The live feed's quality, from the process's own observations
 * (UI-BRIEF §4.7 /data, for the data this host has): gaps between
 * observations, time going backwards, and how far behind the venue's
 * clock the process saw each one.
 */
export function DataQuality() {
  const journals = useQuery({ queryKey: ["journals"], queryFn: api.journals });
  const id = [...(journals.data ?? [])].map((j) => j.id).sort().reverse()[0];
  const ticks = useQuery({ queryKey: ["records", id, "tick", 1000], queryFn: () => api.records(id, ["tick"], 1000), enabled: Boolean(id), refetchInterval: 30_000 });
  const status = useQuery({ queryKey: ["ops", "status"], queryFn: api.traderStatus, retry: false });

  const stats = useMemo(() => {
    const rs = [...(ticks.data?.records ?? [])].reverse();
    if (rs.length < 2) return null;
    const seen = rs.map((r) => Number(r.fields.seen));
    const at = rs.map((r) => r.at ?? 0);
    const gaps = seen.slice(1).map((s, i) => (s - seen[i]) / 1e9);
    const lag = seen.map((s, i) => (s - at[i]) / 1e9).sort((a, b) => a - b);
    const backwards = at.slice(1).filter((a, i) => a < at[i]).length;
    const q = (p: number) => lag[Math.min(lag.length - 1, Math.floor(p * lag.length))];
    return {
      n: rs.length,
      span: (seen[seen.length - 1] - seen[0]) / 1e9,
      maxGap: Math.max(...gaps),
      gaps5: gaps.filter((g) => g > 5).length,
      backwards,
      p50: q(0.5),
      p95: q(0.95),
      max: lag[lag.length - 1],
    };
  }, [ticks.data]);

  return (
    <div className="space-y-4">
      <h1 className="text-lg text-ink">数据质量</h1>
      <p className="text-xs text-ink-muted">
        这台主机上的数据是交易进程自己的实时行情。采集主机（capture → ingest、book-check / trade-check）不在本 deck 的管理范围内。
      </p>
      {status.data && (
        <div className="grid gap-3 sm:grid-cols-4">
          <Tile label="深度更新" value={String(status.data.feed.depth)} />
          <Tile label="乱序" value={String(status.data.feed.out_of_order)} sub="交易所时间倒退、被钳到高水位的事件数" />
          <Tile label="重新同步" value={String(status.data.feed.resyncs)} tone={status.data.feed.resyncs > 0 ? "bad" : undefined} />
          <Tile label="读不出" value={String(status.data.feed.unreadable)} tone={status.data.feed.unreadable > 0 ? "bad" : undefined} />
        </div>
      )}
      {ticks.isLoading ? (
        <Skeleton tiles={4} rows={0} />
      ) : ticks.isError ? (
        <ErrorState error={ticks.error} what="行情记录" />
      ) : !stats ? (
        <Empty title="journal 里的行情还不够分析。" next="至少要有两条 tick。" />
      ) : (
        <>
          <h2 className="text-sm text-ink-muted">
            最近 {stats.n} 条 <Term name="tick">tick</Term>（{(stats.span / 60).toFixed(0)} 分钟）
          </h2>
          <div className="grid gap-3 sm:grid-cols-4">
            <Tile label="最长间隔" value={`${stats.maxGap.toFixed(1)} 秒`} tone={stats.maxGap > 10 ? "bad" : undefined} />
            <Tile label="超过 5 秒的间隔" value={String(stats.gaps5)} tone={stats.gaps5 > 0 ? "bad" : undefined} />
            <Tile label="交易所时间倒退" value={String(stats.backwards)} tone={stats.backwards > 0 ? "bad" : undefined} />
            <Tile label="到达延迟 p50 / p95 / 最大" value={`${stats.p50.toFixed(2)} / ${stats.p95.toFixed(2)} / ${stats.max.toFixed(2)} 秒`} />
          </div>
        </>
      )}
    </div>
  );
}
