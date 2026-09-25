import { Suspense, useEffect, useRef, useState, type ComponentType } from "react";
import { Link, NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  BarChart3,
  Bell,
  Boxes,
  ChevronDown,
  FileCog,
  FlaskConical,
  GitCompareArrows,
  History,
  KeyRound,
  LayoutDashboard,
  ListTree,
  LogOut,
  Rocket,
  ScrollText,
  Server,
  Settings as SettingsIcon,
  ShieldCheck,
  Target,
  TerminalSquare,
} from "lucide-react";

import { api, type CapabilityName } from "@/api/client";
import { Skeleton } from "@/components/States";
import { Badge, StatusDot, cx } from "@/ui/kit";

type Item = { to: string; label: string; icon: ComponentType<{ className?: string }>; capability: CapabilityName | null };

/**
 * Navigation is grouped by what the operator is doing, ordered by how
 * often they do it (docs/UI-V4). It is still drawn from the deck's
 * capabilities: an item the deck cannot back has no link, and a group
 * with nothing in it has no heading.
 */
const NAV: { group: string | null; items: Item[] }[] = [
  { group: null, items: [{ to: "/", label: "总览", icon: LayoutDashboard, capability: null }] },
  {
    group: "交易",
    items: [
      { to: "/live", label: "实盘", icon: Activity, capability: "ops" },
      { to: "/reconcile", label: "对账与归因", icon: GitCompareArrows, capability: "live" },
    ],
  },
  {
    group: "诊断",
    items: [
      { to: "/alerts", label: "告警", icon: Bell, capability: "ops" },
      { to: "/blackbox", label: "黑匣子复盘", icon: History, capability: "ops" },
      { to: "/logs", label: "日志", icon: TerminalSquare, capability: "ops" },
      { to: "/journal", label: "事件回放", icon: ListTree, capability: "live" },
    ],
  },
  {
    group: "变更",
    items: [
      { to: "/strategies", label: "策略与上线", icon: Target, capability: "ops" },
      { to: "/config", label: "配置", icon: FileCog, capability: "ops" },
      { to: "/deploy", label: "发布", icon: Rocket, capability: "ops" },
    ],
  },
  {
    group: "研究",
    items: [
      { to: "/runs", label: "回测记录", icon: BarChart3, capability: "runs" },
      { to: "/sweeps", label: "参数扫描", icon: FlaskConical, capability: "runs" },
    ],
  },
  {
    group: "系统",
    items: [
      { to: "/host", label: "主机与服务", icon: Server, capability: "ops" },
      { to: "/accounts", label: "交易所账户", icon: KeyRound, capability: "ops" },
      { to: "/audit", label: "审计日志", icon: ShieldCheck, capability: "ops" },
      { to: "/settings", label: "设置", icon: SettingsIcon, capability: null },
    ],
  },
];

export function Shell() {
  const { data: caps } = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const allowed = (i: Item) => i.capability === null || caps?.[i.capability]?.available === true;
  const location = useLocation();
  const current = NAV.flatMap((g) => g.items).find((i) => (i.to === "/" ? location.pathname === "/" : location.pathname.startsWith(i.to)));

  return (
    <div className="flex h-screen overflow-hidden">
      <nav className="flex w-56 shrink-0 flex-col border-r border-line bg-surface">
        <Link to="/" className="flex items-center gap-2.5 px-5 py-4">
          <span className="flex h-7 w-7 items-center justify-center rounded-md bg-accent/15 text-accent">
            <Boxes className="h-4 w-4" />
          </span>
          <span className="text-[15px] font-semibold tracking-tight text-ink">quanterdeck</span>
        </Link>
        <div className="flex-1 overflow-y-auto px-3 pb-4">
          {NAV.map(({ group, items }) => {
            const visible = items.filter(allowed);
            if (visible.length === 0) return null;
            return (
              <div key={group ?? "top"} className="mt-3 first:mt-1">
                {group && <div className="px-2.5 pb-1.5 pt-2 text-[11px] font-medium uppercase tracking-wider text-ink-faint">{group}</div>}
                <ul className="space-y-0.5">
                  {visible.map((item) => (
                    <li key={item.to}>
                      <NavLink
                        to={item.to}
                        end={item.to === "/"}
                        className={({ isActive }) =>
                          cx(
                            "flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm transition-colors",
                            isActive ? "bg-accent/12 text-ink" : "text-ink-muted hover:bg-surface-hover hover:text-ink",
                          )
                        }
                      >
                        {({ isActive }) => (
                          <>
                            <item.icon className={cx("h-4 w-4", isActive ? "text-accent" : "text-ink-faint")} />
                            {item.label}
                          </>
                        )}
                      </NavLink>
                    </li>
                  ))}
                </ul>
              </div>
            );
          })}
        </div>
        <div className="border-t border-line px-5 py-3 text-[11px] text-ink-faint">
          {caps ? `v${caps.version}` : " "}
        </div>
      </nav>
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar title={current?.label ?? ""} ops={caps?.ops?.available === true} writable={caps?.writes.available === true} />
        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto max-w-[1440px] px-6 py-6">
            <Suspense fallback={<Skeleton tiles={4} rows={6} />}>
              <Outlet />
            </Suspense>
          </div>
        </main>
      </div>
    </div>
  );
}

/**
 * Always in view: which environment, which host, whether the trader is
 * running, how many alerts. None of these should need a trip to the
 * overview to confirm.
 */
function TopBar({ title, ops, writable }: { title: string; ops: boolean; writable: boolean }) {
  const status = useQuery({ queryKey: ["ops", "status"], queryFn: api.traderStatus, refetchInterval: 10_000, retry: false, enabled: ops });
  const host = useQuery({ queryKey: ["ops", "host"], queryFn: api.host, refetchInterval: 60_000, enabled: ops });
  const alerts = useQuery({ queryKey: ["ops", "alerts"], queryFn: api.alerts, refetchInterval: 10_000, enabled: ops });
  const s = status.data;
  const live = s?.deployment === "Live";
  const active = (alerts.data ?? []).filter((a) => !("silenced_until_ms" in a && (a as { silenced_until_ms?: number | null }).silenced_until_ms));

  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-line bg-surface/60 px-6 backdrop-blur">
      <div className="text-sm font-medium text-ink">{title}</div>
      <div className="ml-auto flex items-center gap-2.5">
        {ops && (s || host.data) && (
          <Badge tone={live ? "bad" : "accent"}>
            {live ? "主网" : s ? "测试网" : "环境未知"}
            {host.data?.name && <span className="text-ink-muted">· {host.data.name}</span>}
          </Badge>
        )}
        {ops && (
          <Link to="/live" className="inline-flex items-center gap-2 rounded-full border border-line px-3 py-1 text-xs hover:bg-surface-hover">
            {status.isError ? (
              <>
                <StatusDot tone="bad" />
                <span className="text-bad">交易进程无应答</span>
              </>
            ) : !s ? (
              <span className="text-ink-faint">读取中…</span>
            ) : s.halted ? (
              <>
                <StatusDot tone="bad" />
                <span className="text-bad">已停机</span>
              </>
            ) : (
              <>
                <StatusDot tone="good" pulse />
                <span className="text-ink">交易中</span>
                <span className="text-ink-faint">{s.symbol}</span>
              </>
            )}
          </Link>
        )}
        {ops && (
          <Link to="/alerts" className="relative rounded-md p-1.5 text-ink-muted hover:bg-surface-hover hover:text-ink" title="告警">
            <Bell className="h-4 w-4" />
            {active.length > 0 && (
              <span className="absolute -right-0.5 -top-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-bad px-1 text-[10px] font-semibold text-white">
                {active.length}
              </span>
            )}
          </Link>
        )}
        {!writable && <Badge>只读</Badge>}
        <UserMenu />
      </div>
    </header>
  );
}

function UserMenu() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  useEffect(() => {
    const close = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, []);
  async function logOut() {
    await api.logout().catch(() => undefined);
    await queryClient.invalidateQueries();
    navigate("/login", { replace: true });
  }
  return (
    <div className="relative" ref={ref}>
      <button onClick={() => setOpen(!open)} className="inline-flex items-center gap-1.5 rounded-md px-2 py-1.5 text-sm text-ink-muted hover:bg-surface-hover hover:text-ink">
        <span className="flex h-6 w-6 items-center justify-center rounded-full bg-surface-raised text-xs text-ink">操</span>
        <ChevronDown className="h-3.5 w-3.5" />
      </button>
      {open && (
        <div className="absolute right-0 z-50 mt-1 w-44 rounded-md border border-line-strong bg-surface-raised py-1 shadow-xl">
          <div className="px-3 py-2 text-xs text-ink-faint">已登录（操作者）</div>
          <Link to="/settings" onClick={() => setOpen(false)} className="flex items-center gap-2 px-3 py-1.5 text-sm text-ink hover:bg-surface-hover">
            <SettingsIcon className="h-4 w-4 text-ink-muted" /> 设置
          </Link>
          <Link to="/audit" onClick={() => setOpen(false)} className="flex items-center gap-2 px-3 py-1.5 text-sm text-ink hover:bg-surface-hover">
            <ScrollText className="h-4 w-4 text-ink-muted" /> 审计日志
          </Link>
          <button onClick={logOut} className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-ink hover:bg-surface-hover">
            <LogOut className="h-4 w-4 text-ink-muted" /> 登出
          </button>
        </div>
      )}
    </div>
  );
}
