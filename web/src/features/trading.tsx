import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { OctagonPause, Play, Power } from "lucide-react";

import { api, type OpsAction, type RecordsPage, type TraderStatus } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { intlLocale, tr } from "@/i18n";
import { Button } from "@/ui/kit";

/**
 * What several screens need about the trader: its status, the journal
 * of the run in progress, how a journal record reads, and the actions
 * an operator takes on it. Defined once so the overview, the live page
 * and the top bar cannot disagree.
 */

export function useCaps() {
  return useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
}

export function useTrader(enabled = true) {
  return useQuery({ queryKey: ["ops", "status"], queryFn: api.traderStatus, refetchInterval: 10_000, retry: false, enabled });
}

/** The newest readable journal: the run in progress, since ids carry the start time. */
export function useNewestJournal() {
  const q = useQuery({ queryKey: ["journals"], queryFn: api.journals, refetchInterval: 60_000 });
  const id = (q.data ?? [])
    .filter((j) => j.state === "read")
    .map((j) => j.id)
    .sort()
    .reverse()[0];
  return { ...q, id };
}

/** What each journal record kind is called, in the current language. */
export function kindNames(): Record<string, string> {
  return {
    session_start: tr("启动", "Start"),
    tick: tr("行情", "Tick"),
    submitted: tr("下单", "Submitted"),
    outcome: tr("回报", "Outcome"),
    cancelled: tr("撤单", "Cancelled"),
    fill: tr("成交", "Fill"),
    refused: tr("风控拒绝", "Refused"),
    reconciled: tr("接管仓位", "Adopted"),
    waiting: tr("策略等待", "Waiting"),
    operator: tr("操作者", "Operator"),
    funding: tr("资金费", "Funding"),
  };
}

/** One journal record as a line of text. Text, never markup: these fields come from a venue. */
export function recordText(r: RecordsPage["records"][number], ps: number, qs: number): string {
  const f = r.fields;
  const scale = (v: unknown, s: number) => (typeof v === "number" ? (v / 10 ** s).toFixed(s) : String(v));
  switch (r.kind) {
    case "fill":
      return `${f.side} ${f.qty} @ ${f.price}${f.leg ? ` ${f.leg}` : ""}`;
    case "submitted":
      return `${f.side} ${scale(f.qty, qs)} @ ${f.limit_price === 0 ? tr("市价", "market") : scale(f.limit_price, ps)}${f.leg ? ` ${f.leg}` : ""}${f.reduce_only ? tr(" 平仓", " close") : ""}`;
    case "outcome":
      return `${f.tag} ${f.detail}`;
    case "cancelled":
      return tr("撤单", "Cancelled");
    case "refused":
      return tr(`风控拒绝：${f.breach}`, `Refused: ${f.breach}`);
    case "funding":
      {
        const at = new Date(Number(f.settled_ms)).toLocaleString(intlLocale(), { hour12: false });
        const unverified = f.verified ? "" : tr(" · 实盘持仓复现不出交易所金额", " · the live positions did not reproduce the venue's figure");
        return tr(
          `结算 ${at} · 费率 ${f.rate} · 标记价 ${f.mark} · 实盘 ${f.venue} · 模型 ${f.model}${unverified}`,
          `Settled ${at} · rate ${f.rate} · mark ${f.mark} · live ${f.venue} · model ${f.model}${unverified}`,
        );
      }
    case "operator":
      return tr(`${f.command}：${f.reason} → ${f.outcome}（${f.origin}）`, `${f.command}: ${f.reason} → ${f.outcome} (${f.origin})`);
    case "tick":
      return `last ${scale(f.last, ps)} · bid ${scale(f.bid, ps)} · ask ${scale(f.ask, ps)}`;
    case "reconciled":
      return tr(`接管 ${JSON.stringify(f.legs)}`, `Adopted ${JSON.stringify(f.legs)}`);
    case "waiting":
      return Object.entries(f)
        .map(([k, v]) => `${k} ${v}`)
        .join(", ");
    default:
      return JSON.stringify(f);
  }
}

export type Pending = { title: string; consequence: string; action: OpsAction; highRisk: boolean };

export function haltAction(): Pending {
  return {
    title: tr("停机", "Halt"),
    consequence: tr(
      "停止开新仓，撤掉开仓挂单，保留止盈等平仓单，进程继续看着持仓。出事时用这个。",
      "Stop opening positions and withdraw opening orders; take-profits and other closing orders stay, and the process keeps watching the position. Use this when something is wrong.",
    ),
    action: { action: "halt" },
    highRisk: false,
  };
}
export function resumeAction(): Pending {
  return {
    title: tr("解除停机", "Resume"),
    consequence: tr(
      "清除停机状态，策略恢复下单。进程会先确认最近一次持仓核对一致、日志可写，否则拒绝。",
      "Clear the halt and let the strategy trade again. The process first checks that the last position check agreed and the journal is writable, and refuses otherwise.",
    ),
    action: { action: "resume" },
    highRisk: true,
  };
}
export function shutdownAction(): Pending {
  return {
    title: tr("退出交易进程", "Shut down the trader"),
    consequence: tr(
      "撤掉全部挂单（包括止盈），然后退出且不会自动重启：持仓将无人管理、没有止盈保护。只用于计划内维护。",
      "Withdraw every order, take-profits included, and exit without restarting: the position is left unmanaged and unprotected. For planned maintenance only.",
    ),
    action: { action: "shutdown" },
    highRisk: true,
  };
}

/** Halt, resume and shut down, each through the confirming dialog. */
export function TraderActions({ s, writable, compact }: { s: TraderStatus | undefined; writable: boolean; compact?: boolean }) {
  const [pending, setPending] = useState<Pending | null>(null);
  if (!writable || !s) return null;
  return (
    <>
      {!s.halted && (
        <Button variant="danger" icon={<OctagonPause className="h-4 w-4" />} onClick={() => setPending(haltAction())}>
          {tr("停机", "Halt")}
        </Button>
      )}
      {s.halted && s.resume_allowed && (
        <Button variant="primary" icon={<Play className="h-4 w-4" />} onClick={() => setPending(resumeAction())}>
          {tr("解除停机", "Resume")}
        </Button>
      )}
      {!compact && (
        <Button variant="ghost" icon={<Power className="h-4 w-4" />} onClick={() => setPending(shutdownAction())}>
          {tr("退出进程", "Shut down")}
        </Button>
      )}
      {pending && <ActionDialog {...pending} onClose={() => setPending(null)} />}
    </>
  );
}

/** Lots to the venue's units, when the trader says its scale. */
export function lotsText(lots: number, qtyScale: number | undefined) {
  return qtyScale === undefined ? `${lots} lot` : (lots / 10 ** qtyScale).toString();
}
