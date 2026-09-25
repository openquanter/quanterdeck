import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, BellOff, CheckCircle2, History, ListChecks, Send } from "lucide-react";

import { api, type Alert, type OpsAction } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { ErrorState, Skeleton } from "@/components/States";
import { useCaps } from "@/features/trading";
import { Ago, Badge, Button, Card, Freshness, PageHeader, Table, fmtTime } from "@/ui/kit";

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

type PendingAlertAction = { title: string; consequence: string; action: OpsAction };

/**
 * Alerts (docs/UI-V4 §4.5): what is raised now, with silencing beside
 * each one; what was raised and cleared; the rules the host agent checks;
 * and the channel, with a test send in the header. Edges, not levels: a
 * condition is announced when it starts and when it clears.
 */
export function Alerts() {
  const caps = useCaps();
  const writable = caps.data?.writes.available === true;
  const q = useQuery({ queryKey: ["ops", "alerts", "view"], queryFn: api.alertsView, refetchInterval: 10_000 });
  const [pending, setPending] = useState<PendingAlertAction | null>(null);

  return (
    <div className="space-y-5">
      <PageHeader
        title="告警"
        description="主机代理每 30 秒检查一次；条件出现时触发、消失时恢复，两次都推送到告警频道。"
        meta={<Freshness at={q.dataUpdatedAt} fetching={q.isFetching} staleAfterS={30} onRefresh={() => q.refetch()} />}
        actions={
          writable && (
            <Button
              icon={<Send className="h-4 w-4" />}
              onClick={() =>
                setPending({
                  title: "发送测试消息",
                  consequence: "向 Discord「监控告警」频道发一条测试消息，确认渠道通着。",
                  action: { action: "alert_test" },
                })
              }
            >
              测试告警渠道
            </Button>
          )
        }
      />

      {q.isLoading ? (
        <Skeleton rows={6} />
      ) : q.isError ? (
        <ErrorState error={q.error} what="告警" />
      ) : (
        <>
          <Active alerts={q.data!.active} writable={writable} onAct={setPending} />
          <Card title="最近的触发与恢复" icon={<History className="h-4 w-4" />} bodyClassName="p-0" extra={<span>{q.data!.history.length} 条</span>}>
            {q.data!.history.length === 0 ? (
              <p className="px-4 py-6 text-center text-sm text-ink-faint">主机代理启动以来还没有触发过。</p>
            ) : (
              <div className="max-h-[28rem] overflow-auto">
                <Table head={["时间", "", "告警", "键"]} dense>
                  {q.data!.history.map((h, k) => (
                    <tr key={k}>
                      <td className="whitespace-nowrap font-mono text-xs text-ink-muted">
                        {fmtTime(h.at_ms)}
                        <span className="ml-2 text-ink-faint">
                          <Ago ms={h.at_ms} />
                        </span>
                      </td>
                      <td className="whitespace-nowrap">{h.raised ? <Badge tone="bad">触发</Badge> : <Badge tone="good">恢复</Badge>}</td>
                      <td className="text-ink">{h.message}</td>
                      <td className="font-mono text-xs text-ink-faint">{h.key}</td>
                    </tr>
                  ))}
                </Table>
              </div>
            )}
          </Card>
        </>
      )}

      <div className="grid gap-5 xl:grid-cols-5">
        <Card title="规则" icon={<ListChecks className="h-4 w-4" />} className="xl:col-span-3" bodyClassName="p-0" extra={<span>每 30 秒检查一次</span>}>
          <Table head={["键", "什么时候触发"]}>
            {RULES.map(([k, v]) => (
              <tr key={k}>
                <td className="whitespace-nowrap font-mono text-xs text-ink-muted">{k}</td>
                <td className="text-ink">{v}</td>
              </tr>
            ))}
          </Table>
        </Card>
        <Card title="渠道" icon={<Send className="h-4 w-4" />} className="xl:col-span-2">
          <p className="text-sm leading-relaxed text-ink">Discord「监控告警」频道（与 1.x 同一个机器人）。</p>
          <p className="mt-2 text-sm leading-relaxed text-ink-muted">每一条操作审计也会同步发到这个频道。静默只停推送，页面上照常显示。</p>
          {!writable && <p className="mt-3 text-xs text-ink-faint">写入未开启：测试渠道与静默不可用。</p>}
        </Card>
      </div>

      {pending && <ActionDialog {...pending} highRisk={false} onClose={() => setPending(null)} />}
    </div>
  );
}

/** The alerts raised now, each with its silence action in the row. */
function Active({
  alerts,
  writable,
  onAct,
}: {
  alerts: Alert[];
  writable: boolean;
  onAct: (p: PendingAlertAction) => void;
}) {
  if (alerts.length === 0) {
    return (
      <div className="flex items-center gap-3 rounded-[var(--radius-card)] border border-good/30 bg-good/8 px-4 py-3.5 text-sm">
        <CheckCircle2 className="h-4 w-4 text-good" />
        <span className="text-ink">当前没有告警</span>
      </div>
    );
  }
  return (
    <Card title="当前告警" icon={<AlertTriangle className="h-4 w-4 text-bad" />} tone="bad" bodyClassName="p-0" extra={<Badge tone="bad">{alerts.length}</Badge>}>
      <ul className="divide-y divide-line/60">
        {alerts.map((a) => (
          <li key={a.key} className="flex flex-wrap items-center gap-3 px-4 py-3 text-sm">
            <AlertTriangle className="h-4 w-4 shrink-0 text-bad" />
            <div className="min-w-0 flex-1">
              <div className="text-ink">{a.message}</div>
              <div className="mt-0.5 text-xs text-ink-muted">
                自 {fmtTime(a.since_ms)}（<Ago ms={a.since_ms} />）<span className="ml-2 font-mono text-ink-faint">{a.key}</span>
              </div>
            </div>
            {a.silenced_until_ms ? (
              <Badge tone="warn">
                <BellOff className="h-3 w-3" />
                静默至 {fmtTime(a.silenced_until_ms, false)}
              </Badge>
            ) : (
              writable && (
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<BellOff className="h-3.5 w-3.5" />}
                  onClick={() =>
                    onAct({
                      title: `静默「${a.message}」1 小时`,
                      consequence: "1 小时内这条告警的触发与恢复都不推送；页面上照常显示。",
                      action: { action: "alert_silence", key: a.key, minutes: 60 },
                    })
                  }
                >
                  静默 1 小时
                </Button>
              )
            )}
          </li>
        ))}
      </ul>
    </Card>
  );
}
