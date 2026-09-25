import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, Loader2, Package, RotateCcw, Rocket, ShieldAlert } from "lucide-react";

import { api, type OpsAction, type Releases } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { useCaps } from "@/features/trading";
import { Ago, Badge, Button, Card, Freshness, KV, PageHeader, Table, cx, fmtDuration, fmtTime, type Tone } from "@/ui/kit";

type PendingAct = { title: string; consequence: string; action: OpsAction };

/**
 * Releases (docs/UI-V4 §4.5 发布): what runs now with its rollback beside
 * it, the signed artefacts waiting to go out each with its own deploy
 * button, and the last deployment's steps as a timeline. The host agent
 * verifies signatures and runs the switch; this screen only asks it to.
 */
export function Deploy() {
  const caps = useCaps();
  const writable = caps.data?.writes.available === true;
  const rel = useQuery({ queryKey: ["ops", "releases"], queryFn: api.releases, refetchInterval: 5_000 });
  const [pending, setPending] = useState<PendingAct | null>(null);
  const r = rel.data;

  return (
    <div className="space-y-5">
      <PageHeader
        title="发布"
        description="交易主机上运行的版本、待部署的签名构件、最近一次部署的每一步。部署与回滚都会先停交易进程。"
        meta={<Freshness at={rel.dataUpdatedAt} fetching={rel.isFetching} staleAfterS={30} onRefresh={() => rel.refetch()} />}
      />
      {rel.isLoading ? (
        <Skeleton tiles={0} rows={6} />
      ) : rel.isError ? (
        <ErrorState error={rel.error} what="发布信息" />
      ) : r ? (
        <>
          <Current r={r} writable={writable} onAct={setPending} />
          <Staged r={r} writable={writable} onAct={setPending} />
          {r.progress.id && <Progress r={r} />}
        </>
      ) : null}
      {pending && <ActionDialog {...pending} highRisk onClose={() => setPending(null)} />}
    </div>
  );
}

function Current({ r, writable, onAct }: { r: Releases; writable: boolean; onAct: (p: PendingAct) => void }) {
  // The newest step of a finished deployment of the running release is
  // when it went live; anything else we cannot date.
  const since = !r.progress.running && r.progress.id === r.current && r.progress.steps.length ? r.progress.steps[r.progress.steps.length - 1].at_ms : null;
  return (
    <Card
      title="当前运行"
      icon={<Package className="h-4 w-4" />}
      extra={
        writable &&
        r.previous &&
        !r.progress.running && (
          <Button
            size="sm"
            variant="danger"
            icon={<RotateCcw className="h-3.5 w-3.5" />}
            onClick={() =>
              onAct({
                title: `回滚到 ${r.previous}`,
                consequence: "停止交易进程（撤掉全部挂单），切回上一个版本并启动；健康检查不过会再切回来。",
                action: { action: "rollback" },
              })
            }
          >
            回滚到 {r.previous}…
          </Button>
        )
      }
    >
      <div className="flex flex-wrap items-center gap-3">
        <span className="font-mono text-2xl font-semibold tracking-tight text-ink">{r.current ?? "—"}</span>
        {r.progress.running && <Badge tone="accent">部署进行中</Badge>}
        {!r.current && <span className="text-sm text-ink-muted">主机代理没有报告当前版本。</span>}
      </div>
      <div className="mt-4">
        <KV
          cols={3}
          items={[
            ["部署时间", since ? <Ago key="a" ms={since} /> : "—"],
            ["上一个版本", <span key="p" className="font-mono">{r.previous ?? "—"}</span>],
            ["已安装版本", String(r.installed.length)],
          ]}
        />
      </div>
      {r.installed.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {r.installed.map((id) => (
            <Badge key={id} tone={id === r.current ? "accent" : "neutral"}>
              <span className="font-mono">{id}</span>
            </Badge>
          ))}
        </div>
      )}
    </Card>
  );
}

function Staged({ r, writable, onAct }: { r: Releases; writable: boolean; onAct: (p: PendingAct) => void }) {
  return (
    <Card title={`已暂存的构件（${r.staged.length}）`} icon={<Rocket className="h-4 w-4" />} extra="本机签名的构件" bodyClassName="p-0">
      {r.staged.length === 0 ? (
        <div className="p-4">
          <Empty
            title="没有待部署的构件。"
            next={
              <>
                在本机运行发布仓的 <code className="font-mono">ops/release.sh</code> 构建、签名并上传。
              </>
            }
          />
        </div>
      ) : (
        <Table head={["构件", "校验", "文件", ""]}>
          {r.staged.map((s) => (
            <tr key={s.id}>
              <td className="whitespace-nowrap font-mono text-xs text-ink">
                {s.id}
                {s.id === r.current && (
                  <span className="ml-2">
                    <Badge tone="accent">正在运行</Badge>
                  </span>
                )}
              </td>
              <td>
                {s.verified ? (
                  <Badge tone="good" dot>
                    签名与校验和通过
                  </Badge>
                ) : (
                  <span className="inline-flex items-center gap-1.5 text-bad">
                    <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
                    不可部署：{s.problem}
                  </span>
                )}
              </td>
              <td className="max-w-md truncate text-xs text-ink-muted" title={s.manifest ? Object.keys(s.manifest.files).join(", ") : undefined}>
                {s.manifest ? Object.keys(s.manifest.files).join(", ") : "—"}
              </td>
              <td className="text-right">
                {writable && s.verified && !r.progress.running && s.id !== r.current && (
                  <Button
                    size="sm"
                    variant="danger"
                    icon={<Rocket className="h-3.5 w-3.5" />}
                    onClick={() =>
                      onAct({
                        title: `部署 ${s.id}`,
                        consequence: "停止交易进程（撤掉全部挂单），切换到这个版本并启动；5 分钟内健康检查（行情、未停机、持仓不变）不通过会自动回滚。",
                        action: { action: "deploy", id: s.id },
                      })
                    }
                  >
                    部署…
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </Table>
      )}
    </Card>
  );
}

/**
 * The outcome in the agent's own words, with a tone only where the words
 * are unambiguous; anything else stays neutral rather than guessing.
 */
function outcomeTone(outcome: string): Tone {
  // The agent's words (oq-agent deploy.rs): "… is running and healthy"
  // on success; "unhealthy", "rolled back", "could not …", "failed" otherwise.
  if (/unhealthy|roll(ed)? back|could not|fail|refused|error/i.test(outcome)) return "bad";
  if (/running and healthy/i.test(outcome)) return "good";
  return "neutral";
}

function Progress({ r }: { r: Releases }) {
  const p = r.progress;
  const steps = p.steps;
  const tone = p.running ? "accent" : p.outcome ? outcomeTone(p.outcome) : "neutral";
  const span = steps.length > 1 ? (steps[steps.length - 1].at_ms - steps[0].at_ms) / 1000 : null;
  return (
    <Card
      title={
        <>
          {p.running ? "进行中" : "最近一次部署"}：<span className="font-mono">{p.id}</span>
        </>
      }
      extra={
        <>
          {span !== null && <span>用时 {fmtDuration(span)}</span>}
          {p.running ? (
            <Badge tone="accent">
              <Loader2 className="h-3 w-3 animate-spin" />
              进行中
            </Badge>
          ) : p.outcome ? (
            <Badge tone={tone} dot>
              {p.outcome}
            </Badge>
          ) : null}
        </>
      }
    >
      {steps.length === 0 ? (
        <p className="text-sm text-ink-muted">还没有步骤记录。</p>
      ) : (
        <ol className="relative ml-1.5 border-l border-line-strong">
          {steps.map((s, i) => {
            const last = i === steps.length - 1;
            const active = last && p.running;
            return (
              <li key={i} className="relative pb-4 pl-5 last:pb-0">
                <span
                  className={cx(
                    "absolute -left-[5px] top-1.5 h-2.5 w-2.5 rounded-full ring-4 ring-surface",
                    active ? "animate-pulse bg-accent" : last && !p.running && tone !== "neutral" ? (tone === "good" ? "bg-good" : "bg-bad") : "bg-ink-faint",
                  )}
                />
                <div className="flex flex-wrap items-baseline gap-x-3">
                  <span className="font-mono text-xs tabular-nums text-ink-faint" title={fmtTime(s.at_ms)}>
                    {fmtTime(s.at_ms, false)}
                  </span>
                  <span className={cx("text-sm", active ? "text-ink" : "text-ink-muted")}>{s.step}</span>
                  {i > 0 && <span className="text-xs text-ink-faint">+{fmtDuration((s.at_ms - steps[i - 1].at_ms) / 1000)}</span>}
                </div>
              </li>
            );
          })}
        </ol>
      )}
      {p.outcome && (
        <p className="mt-4 flex items-center gap-2 border-t border-line pt-3 text-sm text-ink">
          {tone === "good" && <CheckCircle2 className="h-4 w-4 text-good" />}
          结果：{p.outcome}
        </p>
      )}
    </Card>
  );
}
