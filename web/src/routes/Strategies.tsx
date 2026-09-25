import { useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { api, type OpsAction, type Stage, type StrategyView } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Empty, ErrorState, Skeleton } from "@/components/States";

const STAGES: { key: Stage; label: string; meaning: string }[] = [
  { key: "draft", label: "草稿", meaning: "还没有证据" },
  { key: "backtested", label: "已回测", meaning: "有一份通过的回测支撑当前配置" },
  { key: "observing", label: "观察中", meaning: "在测试网上跑，积累观察时长和成交" },
  { key: "confirmed", label: "已确认", meaning: "观察期满，有人签字" },
  { key: "live", label: "实盘", meaning: "允许上实盘" },
];

/**
 * The road from draft to live (UI-BRIEF §6, blueprint P2 8): one step at
 * a time, never skipped, with the reason a step is closed written beside
 * it rather than hidden in a tooltip. A configuration change voids the
 * evidence and sends the instance back to draft — shown as what it is.
 */
export function Strategies() {
  const caps = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const writable = caps.data?.writes.available === true;
  const list = useQuery({ queryKey: ["ops", "strategies"], queryFn: api.strategies, refetchInterval: 60_000 });
  const configs = useQuery({ queryKey: ["ops", "configs"], queryFn: api.configs });
  const runs = useQuery({ queryKey: ["runs"], queryFn: api.runs });
  const [pending, setPending] = useState<{ title: string; consequence: string; action: OpsAction } | null>(null);
  const [newName, setNewName] = useState("");
  const [newConfig, setNewConfig] = useState("");

  return (
    <div className="space-y-6">
      <h1 className="text-lg text-ink">策略与上线门控</h1>
      {list.isLoading ? (
        <Skeleton rows={6} />
      ) : list.isError ? (
        <ErrorState error={list.error} what="策略实例" />
      ) : !list.data?.length ? (
        <Empty title="还没有策略实例。" next="在下面用一份配置文件建一个实例，它从草稿开始，一步步走到实盘。" />
      ) : (
        list.data.map((s) => (
          <Instance
            key={s.instance.id}
            s={s}
            writable={writable}
            runs={(runs.data?.entries ?? []).filter((e) => e.state === "read").map((e) => e.id)}
            onAct={setPending}
          />
        ))
      )}

      {writable && (
        <section className="rounded border border-line p-3">
          <h2 className="mb-2 text-sm text-ink-muted">新建实例</h2>
          <div className="flex flex-wrap items-end gap-2 text-xs">
            <label className="text-ink-muted">
              名称
              <input className="mt-1 block rounded border border-line bg-ground p-1.5" value={newName} onChange={(e) => setNewName(e.target.value)} />
            </label>
            <label className="text-ink-muted">
              配置文件
              <select className="mt-1 block rounded border border-line bg-ground p-1.5 font-mono" value={newConfig} onChange={(e) => setNewConfig(e.target.value)}>
                <option value="">选择…</option>
                {configs.data?.map((c) => (
                  <option key={c.name}>{c.name}</option>
                ))}
              </select>
            </label>
            <button
              className="rounded border border-line px-3 py-1.5 text-ink disabled:opacity-40"
              disabled={!newName.trim() || !newConfig}
              onClick={() =>
                setPending({
                  title: `新建实例 ${newName}`,
                  consequence: "实例从草稿开始，没有任何证据。",
                  action: { action: "strategy_create", name: newName.trim(), config: newConfig },
                })
              }
            >
              新建…
            </button>
          </div>
        </section>
      )}

      {pending && <ActionDialog {...pending} highRisk onClose={() => setPending(null)} />}
    </div>
  );
}

function Instance({
  s,
  writable,
  runs,
  onAct,
}: {
  s: StrategyView;
  writable: boolean;
  runs: string[];
  onAct: (p: { title: string; consequence: string; action: OpsAction }) => void;
}) {
  const i = s.instance;
  const at = STAGES.findIndex((x) => x.key === i.stage);
  const [run, setRun] = useState(i.backtest_run ?? "");
  const [passed, setPassed] = useState(false);
  const voided = [...i.history].reverse().find((h) => h.to === "draft" && h.from !== "draft");
  const nextLabel = STAGES.find((x) => x.key === s.next)?.label;

  return (
    <section className="rounded border border-line bg-surface p-4">
      <div className="mb-3 flex items-baseline gap-3">
        <h2 className="text-base text-ink">{i.name}</h2>
        <span className="font-mono text-xs text-ink-muted">{i.id} · {i.config}</span>
      </div>

      <ol className="grid grid-cols-5 gap-1">
        {STAGES.map((stage, k) => (
          <li
            key={stage.key}
            className={[
              "rounded px-2 py-2 text-xs",
              k < at ? "bg-surface-raised text-ink-muted" : k === at ? "border border-accent text-ink" : "border border-dashed border-line text-ink-muted",
            ].join(" ")}
          >
            <div className="text-sm">{stage.label}</div>
            <div className="mt-0.5">{stage.meaning}</div>
          </li>
        ))}
      </ol>

      {voided && i.stage === "draft" && (
        <p className="mt-3 rounded border border-warn bg-warn/10 px-3 py-2 text-sm text-ink">
          ↩ {new Date(voided.at_ms).toLocaleString("zh-CN", { hour12: false })} 从「{STAGES.find((x) => x.key === voided.from)?.label}」退回草稿：{voided.reason}
        </p>
      )}

      <div className="mt-3 grid gap-2 text-xs text-ink-muted sm:grid-cols-4">
        <div>回测：{i.backtest_run ? `${i.backtest_run}（${i.backtest_passed ? "通过" : "未通过"}）` : "无"}</div>
        <div>观察：{s.evidence.observation_hours} / {s.evidence.required_hours} 小时</div>
        <div>观察期成交：{s.evidence.observation_fills} / 至少 {s.evidence.required_fills}</div>
        <div>确认人：{i.confirmed_by ?? "无"}</div>
      </div>

      {s.next && (
        <div className="mt-3 flex flex-wrap items-center gap-3">
          {writable && (
            <button
              className="rounded border border-accent px-3 py-1 text-xs text-accent disabled:border-line disabled:text-ink-muted"
              disabled={!s.decision.allowed}
              onClick={() =>
                onAct({
                  title: `把 ${i.name} 推进到「${nextLabel}」`,
                  consequence:
                    s.next === "confirmed"
                      ? "这一步就是你的签字确认：观察期的结果你已经看过。"
                      : s.next === "live"
                        ? "只记录门控状态，不会自动部署到实盘主机。"
                        : `进入「${nextLabel}」。`,
                  action: { action: "strategy_advance", id: i.id },
                })
              }
            >
              推进到「{nextLabel}」…
            </button>
          )}
          {/* The reason beside the control, not in a tooltip (UI-BRIEF §6). */}
          <span className={`text-sm ${s.decision.allowed ? "text-good" : "text-warn"}`}>
            {s.decision.allowed ? "条件已满足" : translate(s.decision.reason)}
          </span>
        </div>
      )}

      {writable && i.stage === "draft" && (
        <div className="mt-3 flex flex-wrap items-end gap-2 text-xs">
          <label className="text-ink-muted">
            支撑当前配置的回测 run
            <select className="mt-1 block rounded border border-line bg-ground p-1.5 font-mono" value={run} onChange={(e) => setRun(e.target.value)}>
              <option value="">选择…</option>
              {runs.map((r) => (
                <option key={r}>{r}</option>
              ))}
            </select>
          </label>
          <label className="flex items-center gap-1 text-ink">
            <input type="checkbox" checked={passed} onChange={(e) => setPassed(e.target.checked)} />
            我看过结果，判定通过
          </label>
          <button
            className="rounded border border-line px-3 py-1.5 text-ink disabled:opacity-40"
            disabled={!run}
            onClick={() =>
              onAct({
                title: `为 ${i.name} 记录回测 ${run}`,
                consequence: "记下当前配置文件的指纹；之后配置一改，这份证据就作废。",
                action: { action: "strategy_backtest", id: i.id, run, passed },
              })
            }
          >
            记录回测…
          </button>
        </div>
      )}

      <details className="mt-3 text-xs">
        <summary className="cursor-pointer text-ink-muted">经过（{i.history.length}）</summary>
        <ul className="mt-1 space-y-0.5">
          {[...i.history].reverse().map((h, k) => (
            <li key={k} className="text-ink-muted">
              {new Date(h.at_ms).toLocaleString("zh-CN", { hour12: false })} · {h.actor} · {STAGES.find((x) => x.key === h.from)?.label} → {STAGES.find((x) => x.key === h.to)?.label} · {h.reason}
            </li>
          ))}
        </ul>
      </details>
    </section>
  );
}

/** The gate speaks English; the operator reads Chinese. */
function translate(reason: string) {
  const m = reason.match(/^(\d+)h of the (\d+)h observation window remain$/);
  if (m) return `${m[2]} 小时的观察期还剩 ${m[1]} 小时`;
  const f = reason.match(/^observation produced (\d+) fills; at least (\d+) is required/);
  if (f) return `观察期产生 ${f[1]} 笔成交；至少需要 ${f[2]} 笔才能说明这个策略做过任何事`;
  const r = reason.match(/^run (.+) did not pass/);
  if (r) return `回测 ${r[1]} 没有通过；先看结果再推进`;
  return (
    {
      "no backtest has been run for this configuration": "还没有为这套配置跑过回测",
      "nobody has signed off": "没有人签字确认",
      "already live": "已经是实盘",
    } as Record<string, string>
  )[reason] ?? reason;
}
