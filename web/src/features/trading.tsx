import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { OctagonPause, Play, Power } from "lucide-react";

import { api, type OpsAction, type RecordsPage, type TraderStatus } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
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

export const KIND_NAMES: Record<string, string> = {
  session_start: "启动",
  tick: "行情",
  submitted: "下单",
  outcome: "回报",
  cancelled: "撤单",
  fill: "成交",
  refused: "风控拒绝",
  reconciled: "接管仓位",
  waiting: "策略等待",
  operator: "操作者",
  funding: "资金费",
};

/** One journal record as a line of text. Text, never markup: these fields come from a venue. */
export function recordText(r: RecordsPage["records"][number], ps: number, qs: number): string {
  const f = r.fields;
  const scale = (v: unknown, s: number) => (typeof v === "number" ? (v / 10 ** s).toFixed(s) : String(v));
  switch (r.kind) {
    case "fill":
      return `${f.side} ${f.qty} @ ${f.price}${f.leg ? ` ${f.leg}` : ""}`;
    case "submitted":
      return `${f.side} ${scale(f.qty, qs)} @ ${f.limit_price === 0 ? "市价" : scale(f.limit_price, ps)}${f.leg ? ` ${f.leg}` : ""}${f.reduce_only ? " 平仓" : ""}`;
    case "outcome":
      return `${f.tag} ${f.detail}`;
    case "cancelled":
      return "撤单";
    case "refused":
      return `风控拒绝：${f.breach}`;
    case "funding":
      return `结算 ${new Date(Number(f.settled_ms)).toLocaleString("zh-CN", { hour12: false })} · 费率 ${f.rate} · 标记价 ${f.mark} · 实盘 ${f.venue} · 模型 ${f.model}${f.verified ? "" : " · 实盘持仓复现不出交易所金额"}`;
    case "operator":
      return `${f.command}：${f.reason} → ${f.outcome}（${f.origin}）`;
    case "tick":
      return `last ${scale(f.last, ps)} · bid ${scale(f.bid, ps)} · ask ${scale(f.ask, ps)}`;
    case "reconciled":
      return `接管 ${JSON.stringify(f.legs)}`;
    case "waiting":
      return Object.entries(f)
        .map(([k, v]) => `${k} ${v}`)
        .join(", ");
    default:
      return JSON.stringify(f);
  }
}

export type Pending = { title: string; consequence: string; action: OpsAction; highRisk: boolean };

export const HALT: Pending = {
  title: "停机",
  consequence: "停止开新仓，撤掉开仓挂单，保留止盈等平仓单，进程继续看着持仓。出事时用这个。",
  action: { action: "halt" },
  highRisk: false,
};
export const RESUME: Pending = {
  title: "解除停机",
  consequence: "清除停机状态，策略恢复下单。进程会先确认最近一次持仓核对一致、日志可写，否则拒绝。",
  action: { action: "resume" },
  highRisk: true,
};
export const SHUTDOWN: Pending = {
  title: "退出交易进程",
  consequence: "撤掉全部挂单（包括止盈），然后退出且不会自动重启：持仓将无人管理、没有止盈保护。只用于计划内维护。",
  action: { action: "shutdown" },
  highRisk: true,
};

/** Halt, resume and shut down, each through the confirming dialog. */
export function TraderActions({ s, writable, compact }: { s: TraderStatus | undefined; writable: boolean; compact?: boolean }) {
  const [pending, setPending] = useState<Pending | null>(null);
  if (!writable || !s) return null;
  return (
    <>
      {!s.halted && (
        <Button variant="danger" icon={<OctagonPause className="h-4 w-4" />} onClick={() => setPending(HALT)}>
          停机
        </Button>
      )}
      {s.halted && s.resume_allowed && (
        <Button variant="primary" icon={<Play className="h-4 w-4" />} onClick={() => setPending(RESUME)}>
          解除停机
        </Button>
      )}
      {!compact && (
        <Button variant="ghost" icon={<Power className="h-4 w-4" />} onClick={() => setPending(SHUTDOWN)}>
          退出进程
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
