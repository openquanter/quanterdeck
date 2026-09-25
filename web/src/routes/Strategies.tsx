import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowRight, Check, FlaskConical, Lock, Plus, Undo2 } from "lucide-react";

import { api, type OpsAction, type Stage, type StrategyView } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Empty, ErrorState, Expert, Skeleton } from "@/components/States";
import { useCaps } from "@/features/trading";
import { Badge, Button, Card, Drawer, Freshness, KV, PageHeader, Stat, cx, fmtTime } from "@/ui/kit";

const STAGES: { key: Stage; label: string; meaning: string }[] = [
  { key: "draft", label: "草稿", meaning: "还没有证据" },
  { key: "backtested", label: "已回测", meaning: "有一份通过的回测支撑当前配置" },
  { key: "observing", label: "观察中", meaning: "在测试网上跑，积累观察时长和成交" },
  { key: "confirmed", label: "已确认", meaning: "观察期满，有人签字" },
  { key: "live", label: "实盘", meaning: "允许上实盘" },
];

const stageLabel = (s: Stage | null | undefined) => STAGES.find((x) => x.key === s)?.label ?? String(s);

type PendingAct = { title: string; consequence: string; action: OpsAction };

const FIELD = "mt-1.5 block w-full rounded-md border border-line-strong bg-ground px-2.5 py-1.5 text-sm text-ink outline-none focus:border-accent";

/**
 * The road from draft to live (UI-BRIEF §6, docs/UI-V4 §3 变更): one step
 * at a time, never skipped, with the reason a step is closed written
 * beside the button rather than hidden in a tooltip. A configuration
 * change voids the evidence and sends the instance back to draft — shown
 * as what it is, not as a failure.
 */
export function Strategies() {
  const caps = useCaps();
  const writable = caps.data?.writes.available === true;
  const list = useQuery({ queryKey: ["ops", "strategies"], queryFn: api.strategies, refetchInterval: 60_000 });
  const runs = useQuery({ queryKey: ["runs"], queryFn: api.runs });
  const [pending, setPending] = useState<PendingAct | null>(null);
  const [creating, setCreating] = useState(false);

  const readableRuns = (runs.data?.entries ?? []).filter((e) => e.state === "read").map((e) => e.id);

  return (
    <div className="space-y-5">
      <PageHeader
        title="策略与上线"
        description="每个策略实例从草稿一步步走到实盘：不能跳步，每一步需要什么证据、为什么还不能推进，都写在按钮旁边。"
        meta={<Freshness at={list.dataUpdatedAt} fetching={list.isFetching} staleAfterS={120} onRefresh={() => list.refetch()} />}
        actions={
          writable && (
            <Button variant="primary" icon={<Plus className="h-4 w-4" />} onClick={() => setCreating(true)}>
              新建实例
            </Button>
          )
        }
      />
      {list.isLoading ? (
        <Skeleton rows={6} />
      ) : list.isError ? (
        <ErrorState error={list.error} what="策略实例" />
      ) : !list.data?.length ? (
        <Empty
          title="还没有策略实例。"
          next={writable ? "点页头的「新建实例」，用一份配置文件建一个实例；它从草稿开始，一步步走到实盘。" : "写入模式未开启，不能新建实例。在「设置」里看写入模式与原因。"}
        />
      ) : (
        list.data.map((s) => <Instance key={s.instance.id} s={s} writable={writable} runs={readableRuns} onAct={setPending} />)
      )}

      {creating && (
        <CreateDrawer
          onClose={() => setCreating(false)}
          onSubmit={(p) => {
            setCreating(false);
            setPending(p);
          }}
        />
      )}
      {pending && <ActionDialog {...pending} highRisk onClose={() => setPending(null)} />}
    </div>
  );
}

/** A name and a configuration file: all a new instance starts with. */
function CreateDrawer({ onClose, onSubmit }: { onClose: () => void; onSubmit: (p: PendingAct) => void }) {
  const configs = useQuery({ queryKey: ["ops", "configs"], queryFn: api.configs });
  const [name, setName] = useState("");
  const [config, setConfig] = useState("");
  return (
    <Drawer title="新建策略实例" onClose={onClose} width="max-w-md">
      <div className="space-y-4">
        <p className="text-sm text-ink-muted">实例从草稿开始，没有任何证据。之后为它记录回测、观察、签字，逐步推进。</p>
        <label className="block text-xs text-ink-muted">
          名称
          <input className={FIELD} value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </label>
        <label className="block text-xs text-ink-muted">
          配置文件
          <select className={cx(FIELD, "font-mono")} value={config} onChange={(e) => setConfig(e.target.value)}>
            <option value="">选择…</option>
            {configs.data?.map((c) => (
              <option key={c.name}>{c.name}</option>
            ))}
          </select>
        </label>
        {configs.isError && <ErrorState error={configs.error} what="配置文件列表" />}
        {configs.data && configs.data.length === 0 && <p className="text-xs text-warn">配置目录里还没有文件，先在「配置」里放一份。</p>}
        <div className="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={onClose}>
            取消
          </Button>
          <Button
            variant="primary"
            disabled={!name.trim() || !config}
            onClick={() =>
              onSubmit({
                title: `新建实例 ${name.trim()}`,
                consequence: "实例从草稿开始，没有任何证据。",
                action: { action: "strategy_create", name: name.trim(), config },
              })
            }
          >
            新建…
          </Button>
        </div>
      </div>
    </Drawer>
  );
}

function Instance({ s, writable, runs, onAct }: { s: StrategyView; writable: boolean; runs: string[]; onAct: (p: PendingAct) => void }) {
  const i = s.instance;
  const at = STAGES.findIndex((x) => x.key === i.stage);
  const voided = [...i.history].reverse().find((h) => h.to === "draft" && h.from !== "draft");
  const nextLabel = stageLabel(s.next);
  const ev = s.evidence;
  const configMoved = i.config_sha !== null && s.config_sha_now !== null && i.config_sha !== s.config_sha_now;

  return (
    <Card
      title={
        <span className="flex flex-wrap items-baseline gap-2">
          <span className="text-base">{i.name}</span>
          <span className="font-mono text-xs font-normal text-ink-faint">
            {i.id} · {i.config}
          </span>
        </span>
      }
      extra={<Badge tone={i.stage === "live" ? "accent" : "neutral"}>{stageLabel(i.stage)}</Badge>}
      bodyClassName="space-y-5 p-5"
    >
      <Stepper at={at} />

      {voided && i.stage === "draft" && (
        <div className="flex gap-2.5 rounded-md border border-warn/40 bg-warn/8 px-4 py-3 text-sm">
          <Undo2 className="mt-0.5 h-4 w-4 shrink-0 text-warn" />
          <div>
            <p className="text-ink">
              {fmtTime(voided.at_ms)} 从「{stageLabel(voided.from)}」退回草稿，之前的证据作废。
            </p>
            <p className="mt-0.5 text-ink-muted">{voided.reason}</p>
          </div>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <Stat
          label="回测"
          value={i.backtest_run ? (i.backtest_passed ? "通过" : "未通过") : "无"}
          sub={i.backtest_run ?? "还没有记录支撑当前配置的回测"}
          tone={i.backtest_run && !i.backtest_passed ? "warn" : undefined}
        />
        <Stat label="观察时长" value={`${ev.observation_hours} / ${ev.required_hours} 小时`} sub={i.observing_since_ms ? `自 ${fmtTime(i.observing_since_ms)}` : "尚未开始观察"} />
        <Stat label="观察期成交" value={`${ev.observation_fills} 笔`} sub={`至少 ${ev.required_fills} 笔`} />
        <Stat label="确认人" value={i.confirmed_by ?? "无"} sub={i.confirmed_by ? "已签字" : "观察期满后由人签字"} />
      </div>

      <Expert label="配置指纹">
        <KV
          items={[
            ["记录证据时的配置", <span key="a" className="font-mono text-xs">{i.config_sha ?? "—"}</span>],
            ["配置文件现在", <span key="b" className={cx("font-mono text-xs", configMoved && "text-warn")}>{s.config_sha_now ?? "—"}</span>],
          ]}
        />
        {configMoved && <p className="mt-2 text-xs text-warn">配置已改动：已记下的证据不再支撑当前配置。</p>}
      </Expert>

      {s.next && (
        <div className="flex flex-wrap items-center gap-3 rounded-md border border-line bg-surface-raised/50 px-4 py-3">
          {writable && (
            <Button
              variant="primary"
              icon={<ArrowRight className="h-4 w-4" />}
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
            </Button>
          )}
          {/* The reason beside the control, not in a tooltip (UI-BRIEF §6). */}
          {s.decision.allowed ? (
            <span className="inline-flex items-center gap-1.5 text-sm text-good">
              <Check className="h-4 w-4" />
              条件已满足，可以推进到「{nextLabel}」
            </span>
          ) : (
            <span className="inline-flex items-center gap-1.5 text-sm text-warn">
              <Lock className="h-4 w-4 shrink-0" />
              还不能推进到「{nextLabel}」：{translate(s.decision.reason)}
            </span>
          )}
        </div>
      )}

      {writable && i.stage === "draft" && <BacktestForm s={s} runs={runs} onAct={onAct} />}

      <Expert label={`经过（${i.history.length}）`}>
        {i.history.length === 0 ? (
          <p className="text-xs text-ink-faint">还没有经过任何一步。</p>
        ) : (
          <ol className="space-y-1.5 border-l border-line pl-4 text-xs">
            {[...i.history].reverse().map((h, k) => (
              <li key={k} className="relative">
                <span className="absolute -left-[21px] top-1.5 h-2 w-2 rounded-full bg-ink-faint" />
                <span className="text-ink-faint">{fmtTime(h.at_ms)}</span>
                <span className="mx-2 font-mono text-ink-muted">{h.actor}</span>
                <span className="text-ink">
                  {stageLabel(h.from)} → {stageLabel(h.to)}
                </span>
                <span className="ml-2 text-ink-muted">{h.reason}</span>
              </li>
            ))}
          </ol>
        )}
      </Expert>
    </Card>
  );
}

/**
 * Five stages left to right. Done, current and still ahead look different
 * by shape and weight, not by green and red: an early stage is not a bad
 * one (UI-BRIEF §8).
 */
function Stepper({ at }: { at: number }) {
  const last = STAGES.length - 1;
  // A segment is solid once the stage it leads to has been reached.
  const seg = (reached: boolean) => (reached ? "bg-line-strong" : "border-t border-dashed border-line-strong");
  return (
    <ol className="flex items-start">
      {STAGES.map((stage, k) => {
        const done = k < at;
        const current = k === at;
        return (
          <li key={stage.key} className="relative flex min-w-0 flex-1 flex-col items-center text-center">
            {k > 0 && <span aria-hidden className={cx("absolute left-0 right-1/2 top-4 h-px", seg(k <= at))} />}
            {k < last && <span aria-hidden className={cx("absolute left-1/2 right-0 top-4 h-px", seg(k < at))} />}
            <span
              className={cx(
                "relative z-10 flex h-8 w-8 items-center justify-center rounded-full text-xs font-semibold",
                done && "bg-surface-raised text-ink-muted ring-1 ring-line-strong",
                current && "bg-accent text-white ring-4 ring-accent/20",
                !done && !current && "border border-dashed border-line-strong bg-surface text-ink-faint",
              )}
            >
              {done ? <Check className="h-4 w-4" /> : k + 1}
            </span>
            <span className={cx("mt-2 text-sm", current ? "font-medium text-ink" : done ? "text-ink-muted" : "text-ink-faint")}>{stage.label}</span>
            <span className={cx("mt-0.5 hidden px-2 text-xs sm:block", current ? "text-ink-muted" : "text-ink-faint")}>{stage.meaning}</span>
          </li>
        );
      })}
    </ol>
  );
}

/** Tie a backtest run to the configuration as it is now. */
function BacktestForm({ s, runs, onAct }: { s: StrategyView; runs: string[]; onAct: (p: PendingAct) => void }) {
  const i = s.instance;
  const [run, setRun] = useState(i.backtest_run ?? "");
  const [passed, setPassed] = useState(false);
  return (
    <div className="rounded-md border border-line px-4 py-3">
      <div className="mb-2 flex items-center gap-2 text-sm text-ink">
        <FlaskConical className="h-4 w-4 text-ink-muted" />
        记录回测证据
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <label className="min-w-64 text-xs text-ink-muted">
          支撑当前配置的回测 run
          <select className={cx(FIELD, "font-mono")} value={run} onChange={(e) => setRun(e.target.value)}>
            <option value="">选择…</option>
            {runs.map((r) => (
              <option key={r}>{r}</option>
            ))}
          </select>
        </label>
        <label className="flex h-8 items-center gap-2 text-sm text-ink">
          <input type="checkbox" className="h-4 w-4 accent-[var(--color-accent)]" checked={passed} onChange={(e) => setPassed(e.target.checked)} />
          我看过结果，判定通过
        </label>
        <Button
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
        </Button>
      </div>
      {runs.length === 0 && <p className="mt-2 text-xs text-ink-faint">运行记录目录里还没有可读的 run。</p>}
    </div>
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
    (
      {
        "no backtest has been run for this configuration": "还没有为这套配置跑过回测",
        "nobody has signed off": "没有人签字确认",
        "already live": "已经是实盘",
      } as Record<string, string>
    )[reason] ?? reason
  );
}
