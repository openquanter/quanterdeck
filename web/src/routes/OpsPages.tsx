import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { api, type OpsAction } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Button, Tile, Unknown, time } from "@/routes/Ops";

/** Ticks and lots in the contract's own units, when the trader says how. */
function scaled(value: number | null, scale: number | undefined) {
  if (value === null) return "—";
  if (scale === undefined) return String(value);
  return (value / 10 ** scale).toFixed(scale);
}

/** The orders the trader believes resting, as it tracks them. */
export function OpsOrders() {
  const orders = useQuery({ queryKey: ["ops", "orders"], queryFn: api.orders, refetchInterval: 10_000 });
  const status = useQuery({ queryKey: ["ops", "status"], queryFn: api.traderStatus, retry: false });
  const ps = status.data?.price_scale;
  const qs = status.data?.qty_scale;
  return (
    <div>
      <h1 className="mb-4 text-lg text-ink">挂单</h1>
      {orders.isError ? (
        <Unknown what="挂单" error={orders.error} />
      ) : orders.data && orders.data.orders.length === 0 ? (
        <p className="text-sm text-ink-muted">交易进程认为当前没有挂单。</p>
      ) : (
        <table className="w-full text-sm">
          <thead className="text-left text-xs text-ink-muted">
            <tr>
              <th className="py-1">客户端订单号</th>
              <th>方向</th>
              <th>价格</th>
              <th>数量</th>
              <th>类型</th>
            </tr>
          </thead>
          <tbody>
            {orders.data?.orders.map((o) => (
              <tr key={o.client_id} className="border-t border-line">
                <td className="py-1.5 font-mono text-xs">{o.client_id}</td>
                <td className={o.side === "BUY" ? "text-good" : "text-bad"}>{o.side ?? "—"}</td>
                <td className="font-mono">{scaled(o.price_ticks, ps)}</td>
                <td className="font-mono">{scaled(o.qty_lots, qs)}</td>
                <td className="text-xs text-ink-muted">{o.closing ? "平仓（停机时保留）" : "开仓"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="mt-3 text-xs text-ink-muted">
        这是交易进程自己记的账。与交易所实际挂单的比对见「实盘对账」。
      </p>
    </div>
  );
}

const QUICK = ["HALT", "MISMATCH", "FAIL", "operator", "heartbeat"];

/** A log file's tail, filtered, refreshed while watched. */
export function OpsLogs() {
  const files = useQuery({ queryKey: ["ops", "logs"], queryFn: api.logs, refetchInterval: 60_000 });
  const [name, setName] = useState<string | null>(null);
  const [lines, setLines] = useState(200);
  const [grep, setGrep] = useState("");
  const [follow, setFollow] = useState(true);
  useEffect(() => {
    if (!name && files.data && files.data.length > 0) setName(files.data[0].name);
  }, [files.data, name]);
  const tail = useQuery({
    queryKey: ["ops", "log", name, lines, grep],
    queryFn: () => api.log(name as string, lines, grep),
    enabled: name !== null,
    refetchInterval: follow ? 5_000 : false,
  });
  return (
    <div className="flex h-[calc(100vh-7rem)] flex-col">
      <h1 className="mb-3 text-lg text-ink">日志</h1>
      {files.isError && <Unknown what="日志列表" error={files.error} />}
      <div className="mb-3 flex flex-wrap items-center gap-2 text-sm">
        <select
          className="rounded border border-line bg-ground p-1.5 font-mono text-xs"
          value={name ?? ""}
          onChange={(e) => setName(e.target.value)}
        >
          {files.data?.map((f) => (
            <option key={f.name} value={f.name}>
              {f.name} ({(f.size / 1024).toFixed(0)} KiB)
            </option>
          ))}
        </select>
        <input
          className="w-40 rounded border border-line bg-ground p-1.5 text-xs"
          placeholder="只看包含…"
          value={grep}
          onChange={(e) => setGrep(e.target.value)}
        />
        {QUICK.map((q) => (
          <button
            key={q}
            className={`rounded border px-2 py-1 text-xs ${grep === q ? "border-accent text-accent" : "border-line text-ink-muted"}`}
            onClick={() => setGrep(grep === q ? "" : q)}
          >
            {q}
          </button>
        ))}
        <select
          className="rounded border border-line bg-ground p-1.5 text-xs"
          value={lines}
          onChange={(e) => setLines(Number(e.target.value))}
        >
          {[100, 200, 500, 2000].map((n) => (
            <option key={n} value={n}>
              最后 {n} 行
            </option>
          ))}
        </select>
        <label className="flex items-center gap-1 text-xs text-ink-muted">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} />
          自动刷新
        </label>
      </div>
      {tail.isError ? (
        <Unknown what="日志内容" error={tail.error} />
      ) : (
        <pre className="flex-1 overflow-auto rounded border border-line bg-ground p-3 font-mono text-xs leading-5 text-ink">
          {tail.data?.truncated && <span className="text-ink-muted">（只读取了文件最后 4 MiB）{"\n"}</span>}
          {/* Rendered as text, never as markup: these lines come from a
              venue and a strategy, and are not ours to trust. */}
          {tail.data?.lines.join("\n")}
        </pre>
      )}
    </div>
  );
}

/** Staged releases, what runs, and the last deployment's steps. */
export function OpsDeploy() {
  const caps = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const writable = caps.data?.writes.available === true;
  const rel = useQuery({ queryKey: ["ops", "releases"], queryFn: api.releases, refetchInterval: 5_000 });
  const [pending, setPending] = useState<{ title: string; consequence: string; action: OpsAction } | null>(null);
  const r = rel.data;
  return (
    <div className="space-y-6">
      <h1 className="text-lg text-ink">部署</h1>
      {rel.isError && <Unknown what="发布信息" error={rel.error} />}
      {r && (
        <>
          <div className="grid gap-3 sm:grid-cols-3">
            <Tile label="当前运行" value={r.current ?? "—"} />
            <Tile label="上一个" value={r.previous ?? "—"} />
            <Tile label="已安装" value={String(r.installed.length)} />
          </div>

          <section>
            <div className="mb-2 flex items-center">
              <h2 className="text-sm text-ink-muted">待部署（本机签名的构件）</h2>
              {writable && r.previous && !r.progress.running && (
                <div className="ml-auto">
                  <Button
                    label={`回滚到 ${r.previous}`}
                    onClick={() =>
                      setPending({
                        title: `回滚到 ${r.previous}`,
                        consequence:
                          "停止交易进程（撤掉全部挂单），切回上一个版本并启动；健康检查不过会再切回来。",
                        action: { action: "rollback" },
                      })
                    }
                  />
                </div>
              )}
            </div>
            {r.staged.length === 0 ? (
              <p className="text-sm text-ink-muted">
                没有待部署的构件。在本机运行 oq-private 的 <code>ops/release.sh</code> 构建、签名并上传。
              </p>
            ) : (
              <table className="w-full text-sm">
                <tbody>
                  {r.staged.map((s) => (
                    <tr key={s.id} className="border-t border-line">
                      <td className="py-1.5 font-mono text-xs">{s.id}</td>
                      <td className={s.verified ? "text-good" : "text-bad"}>
                        {s.verified ? "签名与校验和通过" : `不可部署：${s.problem}`}
                      </td>
                      <td className="text-xs text-ink-muted">
                        {s.manifest && Object.keys(s.manifest.files).join(", ")}
                      </td>
                      <td className="text-right">
                        {writable && s.verified && !r.progress.running && s.id !== r.current && (
                          <Button
                            danger
                            label="部署"
                            onClick={() =>
                              setPending({
                                title: `部署 ${s.id}`,
                                consequence:
                                  "停止交易进程（撤掉全部挂单），切换到这个版本并启动；5 分钟内健康检查（行情、未停机、持仓不变）不通过会自动回滚。",
                                action: { action: "deploy", id: s.id },
                              })
                            }
                          />
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>

          {r.progress.id && (
            <section>
              <h2 className="mb-2 text-sm text-ink-muted">
                {r.progress.running ? "进行中" : "最近一次"}：{r.progress.id}
              </h2>
              <ol className="space-y-1 text-xs">
                {r.progress.steps.map((s, i) => (
                  <li key={i} className="font-mono text-ink-muted">
                    {time(s.at_ms)} {s.step}
                  </li>
                ))}
              </ol>
              {r.progress.outcome && <p className="mt-2 text-sm text-ink">{r.progress.outcome}</p>}
            </section>
          )}
        </>
      )}
      {pending && <ActionDialog {...pending} highRisk onClose={() => setPending(null)} />}
    </div>
  );
}

/** The agent's audit trail, and whether its chain still holds. */
export function OpsAudit() {
  const audit = useQuery({ queryKey: ["ops", "audit"], queryFn: () => api.audit(300), refetchInterval: 15_000 });
  const a = audit.data;
  return (
    <div>
      <h1 className="mb-4 text-lg text-ink">审计</h1>
      {audit.isError && <Unknown what="审计记录" error={audit.error} />}
      {a && (
        <>
          <p className={`mb-3 text-sm ${a.chain.intact ? "text-good" : "text-bad"}`}>
            {a.chain.intact ? "哈希链完整" : `哈希链断裂：${a.chain.problem}`}
            <span className="ml-2 text-xs text-ink-muted">每一条也同步发到了告警频道</span>
          </p>
          <table className="w-full text-sm">
            <thead className="text-left text-xs text-ink-muted">
              <tr>
                <th className="py-1">#</th>
                <th>时间</th>
                <th>谁</th>
                <th>操作</th>
                <th>原因</th>
                <th>结果</th>
              </tr>
            </thead>
            <tbody>
              {[...a.entries].reverse().map((e) => (
                <tr key={e.seq} className="border-t border-line align-top">
                  <td className="py-1.5 font-mono text-xs text-ink-muted">{e.seq}</td>
                  <td className="text-xs text-ink-muted">{time(e.at_ms)}</td>
                  <td className="font-mono text-xs">{e.actor}</td>
                  <td>{e.op}</td>
                  <td className="text-ink-muted">{e.reason}</td>
                  <td className={e.result.startsWith("refused") || e.result.startsWith("failed") ? "text-bad" : "text-ink"}>
                    {e.result}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </div>
  );
}

/**
 * What the newest journal says the process holds, against what the venue
 * said at its last reading — no pasting. The verdict is one of three:
 * "cannot tell" is not agreement, and is not shown as such.
 */
export function Live() {
  const q = useQuery({ queryKey: ["live", "latest"], queryFn: api.liveLatest, refetchInterval: 30_000 });
  const d = q.data;
  const verdict = d?.reconciliation.verdict;
  return (
    <div className="space-y-4">
      <h1 className="text-lg text-ink">实盘对账</h1>
      {q.isError && <Unknown what="对账结果" error={q.error} />}
      {d && (
        <>
          <div className="grid gap-3 sm:grid-cols-3">
            <Tile
              label="结论"
              value={verdict === "agree" ? "一致" : verdict === "disagree" ? "不一致" : "无法判断"}
              tone={verdict === "agree" ? "good" : "bad"}
              sub={
                verdict === "cannot_tell"
                  ? "没有差异，但日志不完整（有读不出的帧，或没有接管持仓的记录），不能算一致"
                  : undefined
              }
            />
            <Tile label="交易日志" value={d.reconciliation.journal} />
            <Tile
              label="交易所读数"
              value={`${Math.round(d.record_age_ms / 1000)} 秒前`}
              tone={d.record_age_ms > 5 * 60_000 ? "bad" : undefined}
              sub={d.record_age_ms > 5 * 60_000 ? "读数过旧：oq-recon 可能没在更新" : undefined}
            />
          </div>
          {d.reconciliation.differences.length > 0 && (
            <ul className="space-y-1 rounded border border-bad/50 bg-bad/10 p-3 text-sm">
              {d.reconciliation.differences.map((x, i) => (
                <li key={i}>{x}</li>
              ))}
            </ul>
          )}
          <div className="grid gap-4 sm:grid-cols-2">
            <Side title="进程认为（日志重建）" r={d.reconciliation.believed} />
            <Side title="交易所实际" r={d.reconciliation.venue} />
          </div>
        </>
      )}
    </div>
  );
}

function Side({
  title,
  r,
}: {
  title: string;
  r: { legs: [string, number, number][]; orders: string[] };
}) {
  return (
    <div className="rounded border border-line bg-surface p-3 text-sm">
      <div className="mb-2 text-xs text-ink-muted">{title}</div>
      {r.legs.length === 0 ? (
        <div>无持仓</div>
      ) : (
        r.legs.map(([side, qty, px]) => (
          <div key={side} className="font-mono">
            {side} {qty} @ {px}
          </div>
        ))
      )}
      <div className="mt-2 text-xs text-ink-muted">挂单 {r.orders.length}</div>
      <ul className="mt-1 max-h-48 overflow-auto font-mono text-xs text-ink-muted">
        {r.orders.map((o) => (
          <li key={o}>{o}</li>
        ))}
      </ul>
    </div>
  );
}
