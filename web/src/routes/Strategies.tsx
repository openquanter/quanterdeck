import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ArrowRight, Check, FlaskConical, Lock, Plus, Undo2 } from "lucide-react";

import { api, type OpsAction, type Stage, type StrategyView } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Empty, ErrorState, Expert, Skeleton } from "@/components/States";
import { useCaps } from "@/features/trading";
import { pair, tr } from "@/i18n";
import { Badge, Button, Card, Drawer, Freshness, KV, PageHeader, Stat, cx, fmtTime } from "@/ui/kit";

const stages = (): { key: Stage; label: string; meaning: string }[] => [
  { key: "draft", label: tr("草稿", "Draft"), meaning: tr("还没有证据", "No evidence yet") },
  { key: "backtested", label: tr("已回测", "Backtested"), meaning: tr("有一份通过的回测支撑当前配置", "A passing backtest backs the current config") },
  { key: "observing", label: tr("观察中", "Observing"), meaning: tr("在测试网上跑，积累观察时长和成交", "Runs on testnet, accruing observation time and fills") },
  { key: "confirmed", label: tr("已确认", "Confirmed"), meaning: tr("观察期满，有人签字", "Observation complete, signed off") },
  { key: "live", label: tr("实盘", "Live"), meaning: tr("允许上实盘", "Cleared for live") },
];

const stageLabel = (s: Stage | null | undefined) => stages().find((x) => x.key === s)?.label ?? String(s);

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
        title={tr("策略与上线", "Strategies & go-live")}
        description={tr(
          "每个策略实例从草稿一步步走到实盘：不能跳步，每一步需要什么证据、为什么还不能推进，都写在按钮旁边。",
          "Each strategy instance moves from draft to live one step at a time, never skipping. The evidence each step needs, and why it cannot advance yet, sit beside the button.",
        )}
        meta={<Freshness at={list.dataUpdatedAt} fetching={list.isFetching} staleAfterS={120} onRefresh={() => list.refetch()} />}
        actions={
          writable && (
            <Button variant="primary" icon={<Plus className="h-4 w-4" />} onClick={() => setCreating(true)}>
              {tr("新建实例", "New instance")}
            </Button>
          )
        }
      />
      {list.isLoading ? (
        <Skeleton rows={6} />
      ) : list.isError ? (
        <ErrorState error={list.error} what={tr("策略实例", "strategy instances")} />
      ) : !list.data?.length ? (
        <Empty
          title={tr("还没有策略实例。", "No strategy instances yet.")}
          next={
            writable
              ? tr(
                  "点页头的「新建实例」，用一份配置文件建一个实例；它从草稿开始，一步步走到实盘。",
                  "Click \"New instance\" in the header to create one from a config file; it starts as a draft and moves step by step to live.",
                )
              : tr("写入模式未开启，不能新建实例。在「设置」里看写入模式与原因。", "Write mode is off, so instances cannot be created. See Settings for write mode and why.")
          }
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
    <Drawer title={tr("新建策略实例", "New strategy instance")} onClose={onClose} width="max-w-md">
      <div className="space-y-4">
        <p className="text-sm text-ink-muted">
          {tr(
            "实例从草稿开始，没有任何证据。之后为它记录回测、观察、签字，逐步推进。",
            "An instance starts as a draft with no evidence. Record a backtest, observation and sign-off for it to advance step by step.",
          )}
        </p>
        <label className="block text-xs text-ink-muted">
          {tr("名称", "Name")}
          <input className={FIELD} value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </label>
        <label className="block text-xs text-ink-muted">
          {tr("配置文件", "Config file")}
          <select className={cx(FIELD, "font-mono")} value={config} onChange={(e) => setConfig(e.target.value)}>
            <option value="">{tr("选择…", "Select…")}</option>
            {configs.data?.map((c) => (
              <option key={c.name}>{c.name}</option>
            ))}
          </select>
        </label>
        {configs.isError && <ErrorState error={configs.error} what={tr("配置文件列表", "the config file list")} />}
        {configs.data && configs.data.length === 0 && (
          <p className="text-xs text-warn">{tr("配置目录里还没有文件，先在「配置」里放一份。", "The config directory is empty; add a file under Config first.")}</p>
        )}
        <div className="flex justify-end gap-2 pt-2">
          <Button variant="ghost" onClick={onClose}>
            {tr("取消", "Cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={!name.trim() || !config}
            onClick={() =>
              onSubmit({
                title: tr(`新建实例 ${name.trim()}`, `Create instance ${name.trim()}`),
                consequence: tr("实例从草稿开始，没有任何证据。", "The instance starts as a draft with no evidence."),
                action: { action: "strategy_create", name: name.trim(), config },
              })
            }
          >
            {tr("新建…", "Create…")}
          </Button>
        </div>
      </div>
    </Drawer>
  );
}

function Instance({ s, writable, runs, onAct }: { s: StrategyView; writable: boolean; runs: string[]; onAct: (p: PendingAct) => void }) {
  const i = s.instance;
  const at = stages().findIndex((x) => x.key === i.stage);
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
              {tr(
                `${fmtTime(voided.at_ms)} 从「${stageLabel(voided.from)}」退回草稿，之前的证据作废。`,
                `Sent back to draft from "${stageLabel(voided.from)}" at ${fmtTime(voided.at_ms)}; earlier evidence is void.`,
              )}
            </p>
            <p className="mt-0.5 text-ink-muted">{pair(voided.reason, voided.reason_en)}</p>
          </div>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <Stat
          label={tr("回测", "Backtest")}
          value={i.backtest_run ? (i.backtest_passed ? tr("通过", "Passed") : tr("未通过", "Failed")) : tr("无", "None")}
          sub={i.backtest_run ?? tr("还没有记录支撑当前配置的回测", "No backtest recorded for the current config")}
          tone={i.backtest_run && !i.backtest_passed ? "warn" : undefined}
        />
        <Stat
          label={tr("观察时长", "Observation time")}
          value={tr(`${ev.observation_hours} / ${ev.required_hours} 小时`, `${ev.observation_hours} / ${ev.required_hours} h`)}
          sub={i.observing_since_ms ? tr(`自 ${fmtTime(i.observing_since_ms)}`, `Since ${fmtTime(i.observing_since_ms)}`) : tr("尚未开始观察", "Not observing yet")}
        />
        <Stat
          label={tr("观察期成交", "Observation fills")}
          value={tr(`${ev.observation_fills} 笔`, `${ev.observation_fills}`)}
          sub={tr(`至少 ${ev.required_fills} 笔`, `At least ${ev.required_fills}`)}
        />
        <Stat
          label={tr("确认人", "Signed off by")}
          value={i.confirmed_by ?? tr("无", "None")}
          sub={i.confirmed_by ? tr("已签字", "Signed") : tr("观察期满后由人签字", "Signed by a person once observation completes")}
        />
      </div>

      <Expert label={tr("配置指纹", "Config fingerprint")}>
        <KV
          items={[
            [tr("记录证据时的配置", "Config when evidence was recorded"), <span key="a" className="font-mono text-xs">{i.config_sha ?? "—"}</span>],
            [tr("配置文件现在", "Config file now"), <span key="b" className={cx("font-mono text-xs", configMoved && "text-warn")}>{s.config_sha_now ?? "—"}</span>],
          ]}
        />
        {configMoved && (
          <p className="mt-2 text-xs text-warn">
            {tr("配置已改动：已记下的证据不再支撑当前配置。", "The config has changed: the recorded evidence no longer backs the current config.")}
          </p>
        )}
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
                  title: tr(`把 ${i.name} 推进到「${nextLabel}」`, `Advance ${i.name} to "${nextLabel}"`),
                  consequence:
                    s.next === "confirmed"
                      ? tr("这一步就是你的签字确认：观察期的结果你已经看过。", "This step is your sign-off: you have reviewed the observation results.")
                      : s.next === "live"
                        ? tr("只记录门控状态，不会自动部署到实盘主机。", "Only the gate state is recorded; nothing is deployed to the live host.")
                        : tr(`进入「${nextLabel}」。`, `Moves to "${nextLabel}".`),
                  action: { action: "strategy_advance", id: i.id },
                })
              }
            >
              {tr(`推进到「${nextLabel}」…`, `Advance to "${nextLabel}"…`)}
            </Button>
          )}
          {/* The reason beside the control, not in a tooltip (UI-BRIEF §6). */}
          {s.decision.allowed ? (
            <span className="inline-flex items-center gap-1.5 text-sm text-good">
              <Check className="h-4 w-4" />
              {tr(`条件已满足，可以推进到「${nextLabel}」`, `Requirements met; ready to advance to "${nextLabel}"`)}
            </span>
          ) : (
            <span className="inline-flex items-center gap-1.5 text-sm text-warn">
              <Lock className="h-4 w-4 shrink-0" />
              {tr(`还不能推进到「${nextLabel}」：`, `Cannot advance to "${nextLabel}" yet: `)}
              {pair(s.decision.reason.zh, s.decision.reason.en)}
            </span>
          )}
        </div>
      )}

      {writable && i.stage === "draft" && <BacktestForm s={s} runs={runs} onAct={onAct} />}

      <Expert label={tr(`经过（${i.history.length}）`, `History (${i.history.length})`)}>
        {i.history.length === 0 ? (
          <p className="text-xs text-ink-faint">{tr("还没有经过任何一步。", "No steps taken yet.")}</p>
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
                <span className="ml-2 text-ink-muted">{pair(h.reason, h.reason_en)}</span>
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
  const all = stages();
  const last = all.length - 1;
  // A segment is solid once the stage it leads to has been reached.
  const seg = (reached: boolean) => (reached ? "bg-line-strong" : "border-t border-dashed border-line-strong");
  return (
    <ol className="flex items-start">
      {all.map((stage, k) => {
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
        {tr("记录回测证据", "Record backtest evidence")}
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <label className="min-w-64 text-xs text-ink-muted">
          {tr("支撑当前配置的回测 run", "Backtest run backing the current config")}
          <select className={cx(FIELD, "font-mono")} value={run} onChange={(e) => setRun(e.target.value)}>
            <option value="">{tr("选择…", "Select…")}</option>
            {runs.map((r) => (
              <option key={r}>{r}</option>
            ))}
          </select>
        </label>
        <label className="flex h-8 items-center gap-2 text-sm text-ink">
          <input type="checkbox" className="h-4 w-4 accent-[var(--color-accent)]" checked={passed} onChange={(e) => setPassed(e.target.checked)} />
          {tr("我看过结果，判定通过", "I reviewed the results and judge it passed")}
        </label>
        <Button
          disabled={!run}
          onClick={() =>
            onAct({
              title: tr(`为 ${i.name} 记录回测 ${run}`, `Record backtest ${run} for ${i.name}`),
              consequence: tr("记下当前配置文件的指纹；之后配置一改，这份证据就作废。", "Records the current config file's fingerprint; any later config change voids this evidence."),
              action: { action: "strategy_backtest", id: i.id, run, passed },
            })
          }
        >
          {tr("记录回测…", "Record backtest…")}
        </Button>
      </div>
      {runs.length === 0 && <p className="mt-2 text-xs text-ink-faint">{tr("运行记录目录里还没有可读的 run。", "No readable runs in the runs directory yet.")}</p>}
    </div>
  );
}

