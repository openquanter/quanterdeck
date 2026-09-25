import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

import { ApiError } from "@/api/client";

/**
 * The four states every screen delivers (UI-BRIEF §3): loading keeps the
 * final layout's shape, empty says what to do next, an error says how to
 * fix it with the machine detail folded away, and the normal state.
 */

export function Skeleton({ rows = 6, tiles = 0 }: { rows?: number; tiles?: number }) {
  return (
    <div className="space-y-3" aria-busy="true">
      {tiles > 0 && (
        <div className="grid gap-3 sm:grid-cols-4">
          {Array.from({ length: tiles }).map((_, i) => (
            <div key={i} className="h-16 animate-pulse rounded bg-surface" />
          ))}
        </div>
      )}
      <div className="space-y-2">
        {Array.from({ length: rows }).map((_, i) => (
          <div key={i} className="h-7 animate-pulse rounded bg-surface" />
        ))}
      </div>
    </div>
  );
}

/** Nothing to show, and the next step to take — never just "no data". */
export function Empty({ title, next }: { title: string; next: ReactNode }) {
  return (
    <div className="rounded border border-dashed border-line p-5 text-sm">
      <p className="text-ink">{title}</p>
      <div className="mt-2 text-ink-muted">{next}</div>
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
  if (status === 502) return "主机代理（oq-agent）没有应答：在「运维」里看 oq-agent 服务是否在运行。";
  if (status === 409) return "被主机代理或交易进程拒绝，原因见下。";
  if (status >= 500) return "deck 自己出错了。细节里有机器信息，可以据此排查或报告。";
  return "读取失败，请检查网络连接后重试。";
}

export function ErrorState({ error, what }: { error: unknown; what?: string }) {
  const [open, setOpen] = useState(false);
  const detail = error instanceof ApiError ? error.detail : String(error);
  const status = error instanceof ApiError ? error.status : null;
  return (
    <div className="rounded border border-warn/50 bg-warn/10 p-4 text-sm">
      <p className="text-ink">
        {what ? `无法获取${what}。` : ""}
        {fixFor(error)}
      </p>
      {detail && <p className="mt-1 text-ink-muted">{detail}</p>}
      <button className="mt-2 text-xs text-ink-muted underline" onClick={() => setOpen(!open)}>
        {open ? "收起细节" : "机器细节"}
      </button>
      {open && (
        <pre className="mt-2 overflow-auto rounded bg-ground p-2 font-mono text-xs text-ink-muted">
          {JSON.stringify({ status, detail, message: error instanceof Error ? error.message : String(error) }, null, 2)}
        </pre>
      )}
    </div>
  );
}

// -- novice / expert --------------------------------------------------------

type Mode = "novice" | "expert";
const ModeContext = createContext<{ mode: Mode; setMode: (m: Mode) => void }>({
  mode: "novice",
  setMode: () => undefined,
});

/** Remembered per browser: it is a reading preference, not a setting. */
export function ModeProvider({ children }: { children: ReactNode }) {
  const [mode, setMode] = useState<Mode>(() => {
    try {
      return localStorage.getItem("oq-deck-mode") === "expert" ? "expert" : "novice";
    } catch {
      return "novice";
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem("oq-deck-mode", mode);
    } catch {
      /* private window: the choice lasts this page only */
    }
  }, [mode]);
  return <ModeContext.Provider value={{ mode, setMode }}>{children}</ModeContext.Provider>;
}

export function useMode() {
  return useContext(ModeContext);
}

export function ModeToggle() {
  const { mode, setMode } = useMode();
  return (
    <div className="flex overflow-hidden rounded border border-line text-xs">
      {(["novice", "expert"] as Mode[]).map((m) => (
        <button
          key={m}
          className={`px-2 py-0.5 ${mode === m ? "bg-surface-raised text-ink" : "text-ink-muted"}`}
          onClick={() => setMode(m)}
        >
          {m === "novice" ? "新手" : "专家"}
        </button>
      ))}
    </div>
  );
}

/** Shown only in expert mode: the fields a newcomer does not need yet. */
export function Expert({ children }: { children: ReactNode }) {
  return useMode().mode === "expert" ? <>{children}</> : null;
}

/** One plain sentence per term, shown beside it in novice mode. */
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
};

export function Term({ name, children }: { name: keyof typeof GLOSSARY | string; children?: ReactNode }) {
  const { mode } = useMode();
  const text = GLOSSARY[name];
  return (
    <span title={text}>
      {children ?? name}
      {mode === "novice" && text && <span className="ml-1 text-xs text-ink-muted">（{text}）</span>}
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
