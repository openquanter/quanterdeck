import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, BellOff, CheckCircle2, History, ListChecks, Send } from "lucide-react";

import { api, type Alert, type OpsAction } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { ErrorState, Skeleton } from "@/components/States";
import { useCaps } from "@/features/trading";
import { said, tr } from "@/i18n";
import { Ago, Badge, Button, Card, Freshness, PageHeader, Table, fmtTime } from "@/ui/kit";

const rules = (): [string, string][] => [
  ["halted", tr("交易进程停机", "The trading process halted")],
  ["reconcile", tr("进程内持仓核对与交易所不一致", "In-process position reconciliation disagrees with the venue")],
  ["journal", tr("交易日志写不进去（进程已停止开新单）", "The journal cannot be written (the process has stopped opening orders)")],
  ["feed", tr("行情出现读不出的消息", "The market feed sent a message that cannot be read")],
  ["control", tr("交易进程在运行，控制口却无应答", "The trading process is running but its control port does not answer")],
  [tr("unit:<服务>", "unit:<service>"), tr("受管服务没有在运行（操作者主动停机时不报）", "A managed service is not running (not raised when an operator stopped it)")],
  [tr("disk:<挂载点>", "disk:<mount>"), tr("剩余空间不足 10%", "Less than 10% free space left")],
  ["clock", tr("系统时钟未与 NTP 同步", "The system clock is not synced with NTP")],
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
        title={tr("告警", "Alerts")}
        description={tr(
          "主机代理每 30 秒检查一次；条件出现时触发、消失时恢复，两次都推送到告警频道。",
          "The host agent checks every 30 seconds. An alert is raised when a condition appears and cleared when it goes away; both are pushed to the alert channel.",
        )}
        meta={<Freshness at={q.dataUpdatedAt} fetching={q.isFetching} staleAfterS={30} onRefresh={() => q.refetch()} />}
        actions={
          writable && (
            <Button
              icon={<Send className="h-4 w-4" />}
              onClick={() =>
                setPending({
                  title: tr("发送测试消息", "Send a test message"),
                  consequence: tr(
                    "向已配置的每个告警渠道（Discord、Telegram）各发一条测试消息，确认渠道通着。",
                    "Sends a test message to every configured alert channel (Discord, Telegram) to confirm each one works.",
                  ),
                  action: { action: "alert_test" },
                })
              }
            >
              {tr("测试告警渠道", "Test alert channel")}
            </Button>
          )
        }
      />

      {q.isLoading ? (
        <Skeleton rows={6} />
      ) : q.isError ? (
        <ErrorState error={q.error} what={tr("告警", "alerts")} />
      ) : (
        <>
          <Active alerts={q.data!.active} writable={writable} onAct={setPending} />
          <Card
            title={tr("最近的触发与恢复", "Recently raised and cleared")}
            icon={<History className="h-4 w-4" />}
            bodyClassName="p-0"
            extra={<span>{tr(`${q.data!.history.length} 条`, `${q.data!.history.length} ${q.data!.history.length === 1 ? "entry" : "entries"}`)}</span>}
          >
            {q.data!.history.length === 0 ? (
              <p className="px-4 py-6 text-center text-sm text-ink-faint">{tr("主机代理启动以来还没有触发过。", "Nothing has been raised since the host agent started.")}</p>
            ) : (
              <div className="max-h-[28rem] overflow-auto">
                <Table head={[tr("时间", "Time"), "", tr("告警", "Alert"), tr("键", "Key")]} dense>
                  {q.data!.history.map((h, k) => (
                    <tr key={k}>
                      <td className="whitespace-nowrap font-mono text-xs text-ink-muted">
                        {fmtTime(h.at_ms)}
                        <span className="ml-2 text-ink-faint">
                          <Ago ms={h.at_ms} />
                        </span>
                      </td>
                      <td className="whitespace-nowrap">{h.raised ? <Badge tone="bad">{tr("触发", "Raised")}</Badge> : <Badge tone="good">{tr("恢复", "Cleared")}</Badge>}</td>
                      <td className="text-ink">{said(h)}</td>
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
        <Card
          title={tr("规则", "Rules")}
          icon={<ListChecks className="h-4 w-4" />}
          className="xl:col-span-3"
          bodyClassName="p-0"
          extra={<span>{tr("每 30 秒检查一次", "Checked every 30 seconds")}</span>}
        >
          <Table head={[tr("键", "Key"), tr("什么时候触发", "Raised when")]}>
            {rules().map(([k, v]) => (
              <tr key={k}>
                <td className="whitespace-nowrap font-mono text-xs text-ink-muted">{k}</td>
                <td className="text-ink">{v}</td>
              </tr>
            ))}
          </Table>
        </Card>
        <Card title={tr("渠道", "Channels")} icon={<Send className="h-4 w-4" />} className="xl:col-span-2">
          <Channels channels={q.data?.channels} loaded={q.isSuccess} />
          <p className="mt-3 text-sm leading-relaxed text-ink">
            {tr(
              "告警可发往一个 Discord 频道和一个 Telegram 会话，两者都配置时每条消息各发一份；机器人令牌以 systemd 凭据传入。",
              "Alerts can go to a Discord channel and a Telegram chat; with both configured, each gets every message. Bot tokens arrive as systemd credentials.",
            )}
          </p>
          <p className="mt-2 text-sm leading-relaxed text-ink-muted">
            {tr(
              "每一条操作审计也会同步发出。审计链与 journal 的主机外锚点只读回 Discord：Telegram 机器人读不到会话历史，当不了锚点。静默只停推送，页面上照常显示。",
              "Every audited operator action is also posted. The audit trail's and the journal's off-host anchor is read back from Discord only: a Telegram bot cannot read a chat's history, so it cannot serve as one. Silencing only stops the pushes; alerts still show on this page.",
            )}
          </p>
          {!writable && <p className="mt-3 text-xs text-ink-faint">{tr("写入未开启：测试渠道与静默不可用。", "Writes are off: the channel test and silencing are unavailable.")}</p>}
        </Card>
      </div>

      {pending && <ActionDialog {...pending} highRisk={false} onClose={() => setPending(null)} />}
    </div>
  );
}

/**
 * Which channels the agent delivers to. An agent from before the field
 * existed is "cannot tell", not "none": the two are different facts.
 */
function Channels({ channels, loaded }: { channels: string[] | undefined; loaded: boolean }) {
  if (!loaded) return null;
  if (channels === undefined) {
    return <p className="text-xs text-ink-faint">{tr("这个版本的主机代理不报告配置了哪些渠道。", "This host agent's version does not report which channels are configured.")}</p>;
  }
  if (channels.length === 0) {
    return (
      <Badge tone="warn" dot>
        {tr("未配置任何渠道：告警只打印在主机日志里", "No channel configured: alerts are only printed in the host's log")}
      </Badge>
    );
  }
  const label = (c: string) => (c === "discord" ? "Discord" : c === "telegram" ? "Telegram" : c);
  return (
    <div className="flex flex-wrap items-center gap-2">
      <span className="text-xs text-ink-muted">{tr("已配置：", "Configured:")}</span>
      {channels.map((c) => (
        <Badge key={c} tone="good" dot>
          {label(c)}
        </Badge>
      ))}
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
        <span className="text-ink">{tr("当前没有告警", "No active alerts")}</span>
      </div>
    );
  }
  return (
    <Card title={tr("当前告警", "Active alerts")} icon={<AlertTriangle className="h-4 w-4 text-bad" />} tone="bad" bodyClassName="p-0" extra={<Badge tone="bad">{alerts.length}</Badge>}>
      <ul className="divide-y divide-line/60">
        {alerts.map((a) => (
          <li key={a.key} className="flex flex-wrap items-center gap-3 px-4 py-3 text-sm">
            <AlertTriangle className="h-4 w-4 shrink-0 text-bad" />
            <div className="min-w-0 flex-1">
              <div className="text-ink">{said(a)}</div>
              <div className="mt-0.5 text-xs text-ink-muted">
                {tr("自 ", "Since ")}
                {fmtTime(a.since_ms)}
                {tr("（", " (")}
                <Ago ms={a.since_ms} />
                {tr("）", ")")}
                <span className="ml-2 font-mono text-ink-faint">{a.key}</span>
              </div>
            </div>
            {a.silenced_until_ms ? (
              <Badge tone="warn">
                <BellOff className="h-3 w-3" />
                {tr(`静默至 ${fmtTime(a.silenced_until_ms, false)}`, `Silenced until ${fmtTime(a.silenced_until_ms, false)}`)}
              </Badge>
            ) : (
              writable && (
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<BellOff className="h-3.5 w-3.5" />}
                  onClick={() =>
                    onAct({
                      title: tr(`静默「${said(a)}」1 小时`, `Silence "${said(a)}" for 1 hour`),
                      consequence: tr(
                        "1 小时内这条告警的触发与恢复都不推送；页面上照常显示。",
                        "For 1 hour this alert's raises and clears are not pushed; it still shows on this page.",
                      ),
                      action: { action: "alert_silence", key: a.key, minutes: 60 },
                    })
                  }
                >
                  {tr("静默 1 小时", "Silence 1 hour")}
                </Button>
              )
            )}
          </li>
        ))}
      </ul>
    </Card>
  );
}
