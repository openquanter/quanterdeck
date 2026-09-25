import { useState, type ReactNode } from "react";
import { AlertTriangle, ChevronRight, Inbox } from "lucide-react";

import { ApiError } from "@/api/client";

/**
 * The four states every screen delivers (UI-BRIEF §3): loading keeps the
 * final layout's shape, empty says what to do next, an error says how to
 * fix it with the machine detail folded away, and the normal state.
 */

export function Skeleton({ rows = 6, tiles = 0 }: { rows?: number; tiles?: number }) {
  return (
    <div className="space-y-4" aria-busy="true">
      {tiles > 0 && (
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
          {Array.from({ length: tiles }).map((_, i) => (
            <div key={i} className="h-[88px] animate-pulse rounded-[var(--radius-card)] border border-line bg-surface" />
          ))}
        </div>
      )}
      {rows > 0 && (
        <div className="space-y-2 rounded-[var(--radius-card)] border border-line bg-surface p-4">
          {Array.from({ length: rows }).map((_, i) => (
            <div key={i} className="h-6 animate-pulse rounded bg-surface-raised" style={{ width: `${90 - ((i * 13) % 35)}%` }} />
          ))}
        </div>
      )}
    </div>
  );
}

/** Nothing to show, and the next step to take — never just "no data". */
export function Empty({ title, next }: { title: string; next: ReactNode }) {
  return (
    <div className="flex flex-col items-center rounded-[var(--radius-card)] border border-dashed border-line-strong px-6 py-10 text-center">
      <Inbox className="h-8 w-8 text-ink-faint" />
      <p className="mt-3 text-sm font-medium text-ink">{title}</p>
      <div className="mt-1.5 max-w-lg text-sm text-ink-muted">{next}</div>
    </div>
  );
}

/** How to fix it, from the status the deck answered with. */
function fixFor(error: unknown): string {
  const status = error instanceof ApiError ? error.status : 0;
  const detail = error instanceof ApiError ? error.detail : "";
  if (status === 421) {
    return "这个地址不在本 deck 应答的名字里——这是防 DNS rebinding 的保护，不是故障。若经反向代理访问，把代理的域名（带端口）加进 OQ_DECK_EXTRA_HOSTS 并重启 deck。";
  }
  if (status === 403 && /跨站|来源|origin/i.test(detail)) {
    return "这个写入请求被跨站保护拒绝：它的来源不是本 deck 的页面。这是保护，不是操作失败；从 deck 自己的页面重新操作即可。";
  }
  if (status === 403) return "当前模式不允许这个操作。在「设置」里看写入模式与原因。";
  if (status === 401) return "会话已结束，重新登录后回到这里。";
  if (status === 404 && /OQ_DECK_/.test(detail)) return "这项功能需要的目录或代理还没配置：按下面提示设置对应的环境变量，然后重启 deck。";
  if (status === 404) return "要找的东西不在了：文件可能已被轮换或删除。回到列表重新选择。";
  if (status === 502) return "主机代理（oq-agent）没有应答：在「主机与服务」里看 oq-agent 服务是否在运行。";
  if (status === 409) return "被主机代理或交易进程拒绝，原因见下。";
  if (status >= 500) return "deck 自己出错了。细节里有机器信息，可以据此排查或报告。";
  return "读取失败，请检查网络连接后重试。";
}

export function ErrorState({ error, what }: { error: unknown; what?: string }) {
  const [open, setOpen] = useState(false);
  const detail = error instanceof ApiError ? error.detail : String(error);
  const status = error instanceof ApiError ? error.status : null;
  return (
    <div className="rounded-[var(--radius-card)] border border-warn/40 bg-warn/8 p-4 text-sm">
      <div className="flex gap-2.5">
        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warn" />
        <div className="min-w-0">
          <p className="font-medium text-ink">{what ? `无法获取${what}` : "读取失败"}</p>
          <p className="mt-1 text-ink-muted">{fixFor(error)}</p>
          {detail && <p className="mt-1 break-words text-xs text-ink-faint">{detail}</p>}
          <button className="mt-2 text-xs text-ink-faint hover:text-ink-muted" onClick={() => setOpen(!open)}>
            {open ? "收起细节" : "机器细节"}
          </button>
          {open && (
            <pre className="mt-2 overflow-auto rounded-md bg-ground p-2.5 font-mono text-xs text-ink-muted">
              {JSON.stringify({ status, detail, message: error instanceof Error ? error.message : String(error) }, null, 2)}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Detail a first look does not need, folded away and one click open. It
 * replaced an expert mode that hid the same things behind a setting.
 */
export function Expert({ children, label = "详细" }: { children: ReactNode; label?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button className="inline-flex items-center gap-1 text-xs text-ink-faint hover:text-ink-muted" onClick={() => setOpen(!open)}>
        <ChevronRight className={`h-3.5 w-3.5 transition-transform ${open ? "rotate-90" : ""}`} />
        {open ? `收起${label}` : label}
      </button>
      {open && <div className="mt-2">{children}</div>}
    </div>
  );
}

/** One plain sentence per term, shown on hover. */
export const GLOSSARY: Record<string, string> = {
  run: "一次运行的结果文件：身份（代码、数据、配置的指纹）加全部成交和盈亏。",
  journal: "交易进程在发单前写下的决策日志。进程重启后靠它恢复，也是对账的依据。",
  parity: "同样的数据和配置下，两次运行的成交是否一致。",
  residual: "实盘与模型盈亏之差里，五个成因都解释不了的部分。有成因没测到时，它是「未知」而不是 0。",
  belief: "进程根据自己的 journal 认为自己持有的仓位和挂单。",
  hedged: "账户同时持有多头和空头两条腿；只看净额会藏掉其中一条。",
  undecodable: "journal 里解不开的记录数。大于 0 时重建出的结果有洞，对上了也可能只是碰巧。",
  shadow: "和实盘并行、用同样行情跑的回测模型。它的成交与实盘的差别就是要解释的差额。",
  markout: "成交后若干秒内价格朝哪个方向走，用来判断成交质量。",
  halt: "停止开新仓、撤掉开仓挂单，保留止盈等平仓单。",
  tick: "价格的最小变动单位；这里的价格都是整数个 tick。",
  lots: "数量的最小单位；这里的数量都是整数个 lot。",
  run_pnl: "交易进程自这次启动以来的已实现盈亏，扣除手续费、计入资金费。不是自然日盈亏：进程只知道自己这次运行做了什么。",
  self_check: "交易进程定时向交易所查询持仓，与自己内存里记的对比。不一致说明进程的账和交易所对不上，会触发告警。",
  dsr: "折减夏普（Deflated Sharpe）：考虑到试了多少组参数之后，最好那组的夏普仍然大于 0 的概率。试得越多，偶然跑出好结果越容易，这个概率就越低。",
  pbo: "回测过拟合概率（PBO）：把历史切成若干段，样本内最好的参数在样本外排到后一半的比例。越高，说明「最好」越可能只是碰巧。",
  logit: "每次切分里，样本内最优参数在样本外的相对排名（对数几率）。小于 0 表示它在样本外落到了后一半。",
  degradation: "样本内夏普每高 1，样本外夏普跟着变多少。为负说明样本内越好、样本外越差，是典型的过拟合。",
};

/** A term with its plain-language meaning on hover. */
export function Term({ name, children }: { name: keyof typeof GLOSSARY | string; children?: ReactNode }) {
  const text = GLOSSARY[name];
  if (!text) return <>{children ?? name}</>;
  return (
    <span className="group relative inline-flex cursor-help items-baseline border-b border-dotted border-ink-faint">
      {children ?? name}
      <span className="pointer-events-none absolute left-0 top-full z-40 mt-2 hidden w-72 rounded-md border border-line-strong bg-surface-raised px-3 py-2 text-xs font-normal leading-relaxed text-ink shadow-xl group-hover:block">
        {text}
      </span>
    </span>
  );
}

export function Copy({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      className="ml-2 text-xs text-ink-muted hover:text-ink"
      onClick={() => {
        void navigator.clipboard?.writeText(text).then(() => {
          setDone(true);
          setTimeout(() => setDone(false), 1500);
        });
      }}
    >
      {done ? "已复制" : "复制"}
    </button>
  );
}
