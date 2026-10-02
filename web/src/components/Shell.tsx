import { Suspense, useEffect, useRef, useState, type ComponentType, type ReactNode } from "react";
import { Link, NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  BarChart3,
  Bell,
  ChevronDown,
  FileCog,
  FileText,
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
  Layers,
  Target,
  TerminalSquare,
} from "lucide-react";

import { api, type CapabilityName } from "@/api/client";
import { Skeleton } from "@/components/States";
import { Badge, BrandMark, IconTile, StatusDot, cx, type Hue } from "@/ui/kit";
import { ThemeToggle } from "@/ui/theme";
import { LanguageToggle, tr } from "@/i18n";
import { UpstreamBadge } from "@/features/upstream";

type Item = { to: string; label: string; icon: ComponentType<{ className?: string }>; capability: CapabilityName | null };

/**
 * Navigation is grouped by what the operator is doing, ordered by how
 * often they do it (docs/UI-V4). It is still drawn from the deck's
 * capabilities: an item the deck cannot back has no link, and a group
 * with nothing in it has no heading.
 */
/** Built when drawn, so the labels are in the language of the moment. */
const nav = (): { group: string | null; hue: Hue; items: Item[] }[] => [
  { group: null, hue: "blue", items: [{ to: "/", label: tr("总览", "Overview"), icon: LayoutDashboard, capability: null }] },
  {
    group: tr("交易", "Trading"),
    hue: "blue",
    items: [
      { to: "/live", label: tr("实盘", "Live"), icon: Activity, capability: "ops" },
      { to: "/traders", label: tr("交易进程", "Traders"), icon: Layers, capability: "ops" },
      { to: "/reconcile", label: tr("对账与归因", "Reconciliation"), icon: GitCompareArrows, capability: "live" },
    ],
  },
  {
    group: tr("诊断", "Diagnose"),
    hue: "red",
    items: [
      { to: "/alerts", label: tr("告警", "Alerts"), icon: Bell, capability: "ops" },
      { to: "/blackbox", label: tr("黑匣子复盘", "Black box"), icon: History, capability: "ops" },
      { to: "/reports", label: tr("报告", "Reports"), icon: FileText, capability: "reports" },
      { to: "/logs", label: tr("日志", "Logs"), icon: TerminalSquare, capability: "ops" },
      { to: "/journal", label: tr("事件回放", "Journal replay"), icon: ListTree, capability: "live" },
    ],
  },
  {
    group: tr("变更", "Change"),
    hue: "purple",
    items: [
      { to: "/strategies", label: tr("策略与上线", "Strategies"), icon: Target, capability: "ops" },
      { to: "/config", label: tr("配置", "Configuration"), icon: FileCog, capability: "ops" },
      { to: "/deploy", label: tr("发布", "Releases"), icon: Rocket, capability: "ops" },
    ],
  },
  {
    group: tr("研究", "Research"),
    hue: "teal",
    items: [
      { to: "/runs", label: tr("回测记录", "Runs"), icon: BarChart3, capability: "runs" },
      { to: "/sweeps", label: tr("参数扫描", "Sweeps"), icon: FlaskConical, capability: "runs" },
    ],
  },
  {
    group: tr("系统", "System"),
    hue: "green",
    items: [
      { to: "/host", label: tr("主机与服务", "Host & services"), icon: Server, capability: "ops" },
      { to: "/accounts", label: tr("交易所账户", "Venue accounts"), icon: KeyRound, capability: "ops" },
      { to: "/audit", label: tr("审计日志", "Audit log"), icon: ShieldCheck, capability: "ops" },
      { to: "/settings", label: tr("设置", "Settings"), icon: SettingsIcon, capability: null },
    ],
  },
];

/** Each group's icons in its own colour: where on the map a page is. */
const NAV_ICON: Record<Hue, string> = {
  blue: "text-hue-blue",
  red: "text-hue-red",
  purple: "text-hue-purple",
  teal: "text-hue-teal",
  green: "text-hue-green",
  yellow: "text-hue-yellow",
  pink: "text-hue-pink",
  orange: "text-hue-orange",
};

export function Shell() {
  const { data: caps } = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const allowed = (i: Item) => i.capability === null || caps?.[i.capability]?.available === true;
  const location = useLocation();
  const current = nav().flatMap((g) => g.items.map((i) => ({ ...i, hue: g.hue }))).find((i) =>
    i.to === "/" ? location.pathname === "/" : location.pathname.startsWith(i.to),
  );

  return (
    <div className="flex h-screen overflow-hidden">
      <nav className="flex w-60 shrink-0 flex-col border-r border-line bg-surface">
        <Link to="/" className="flex items-center gap-3 px-5 py-5">
          <BrandMark className="h-8 w-8" />
          <span className="text-[17px] font-medium tracking-tight text-ink">
            quanter<span className="text-accent">deck</span>
          </span>
        </Link>
        <div className="flex-1 overflow-y-auto px-3 pb-4">
          {nav().map(({ group, hue, items }) => {
            const visible = items.filter(allowed);
            if (visible.length === 0) return null;
            return (
              <div key={group ?? "top"} className="mt-3 first:mt-1">
                {group && <div className="px-4 pb-1.5 pt-2 text-[11px] font-medium tracking-wider text-ink-faint">{group}</div>}
                <ul className="space-y-0.5">
                  {visible.map((item) => (
                    <li key={item.to}>
                      <NavLink
                        to={item.to}
                        end={item.to === "/"}
                        className={({ isActive }) =>
                          cx(
                            "flex items-center gap-3 rounded-full px-4 py-2 text-sm transition-colors",
                            isActive ? "bg-accent-soft font-medium text-accent" : "text-ink-muted hover:bg-surface-hover hover:text-ink",
                          )
                        }
                      >
                        {({ isActive }) => (
                          <>
                            <item.icon className={cx("h-[18px] w-[18px]", isActive ? "text-accent" : NAV_ICON[hue])} />
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
        <TopBar
          title={
            current ? (
              <span className="flex items-center gap-2.5">
                <IconTile icon={<current.icon />} hue={current.hue} size="sm" />
                {current.label}
              </span>
            ) : (
              ""
            )
          }
          ops={caps?.ops?.available === true}
          writable={caps?.writes.available === true}
          upstream={caps?.upstream?.available === true}
        />
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
function TopBar({ title, ops, writable, upstream }: { title: ReactNode; ops: boolean; writable: boolean; upstream: boolean }) {
  const status = useQuery({ queryKey: ["ops", "status"], queryFn: api.traderStatus, refetchInterval: 10_000, retry: false, enabled: ops });
  const host = useQuery({ queryKey: ["ops", "host"], queryFn: api.host, refetchInterval: 60_000, enabled: ops });
  const alerts = useQuery({ queryKey: ["ops", "alerts"], queryFn: api.alerts, refetchInterval: 10_000, enabled: ops });
  const s = status.data;
  const live = s?.deployment === "Live";
  const active = (alerts.data ?? []).filter((a) => !("silenced_until_ms" in a && (a as { silenced_until_ms?: number | null }).silenced_until_ms));

  return (
    <header className="flex h-16 shrink-0 items-center gap-3 border-b border-line bg-surface/80 px-6 backdrop-blur">
      <div className="text-base font-medium text-ink">{title}</div>
      <div className="ml-auto flex items-center gap-2.5">
        {ops && (s || host.data) && (
          <Badge tone={live ? "bad" : "accent"}>
            {live ? tr("主网", "Mainnet") : s ? tr("测试网", "Testnet") : tr("环境未知", "Unknown environment")}
            {host.data?.name && <span className="text-ink-muted">· {host.data.name}</span>}
          </Badge>
        )}
        {ops && (
          <Link to="/live" className="inline-flex items-center gap-2 rounded-full border border-line px-3 py-1 text-xs hover:bg-surface-hover">
            {status.isError ? (
              <>
                <StatusDot tone="bad" />
                <span className="text-bad">{tr("交易进程无应答", "Trader not answering")}</span>
              </>
            ) : !s ? (
              <span className="text-ink-faint">{tr("读取中…", "Loading…")}</span>
            ) : s.halted ? (
              <>
                <StatusDot tone="bad" />
                <span className="text-bad">{tr("已停机", "Halted")}</span>
              </>
            ) : (
              <>
                <StatusDot tone="good" pulse />
                <span className="text-ink">{tr("交易中", "Trading")}</span>
                <span className="text-ink-faint">{s.symbol}</span>
              </>
            )}
          </Link>
        )}
        {ops && (
          <Link to="/alerts" className="relative rounded-md p-1.5 text-ink-muted hover:bg-surface-hover hover:text-ink" title={tr("告警", "Alerts")}>
            <Bell className="h-4 w-4" />
            {active.length > 0 && (
              <span className="absolute -right-0.5 -top-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-bad px-1 text-[10px] font-semibold text-white">
                {active.length}
              </span>
            )}
          </Link>
        )}
        <UpstreamBadge available={upstream} />
        {!writable && <Badge>{tr("只读", "Read-only")}</Badge>}
        <LanguageToggle />
        <ThemeToggle />
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
        <span className="flex h-8 w-8 items-center justify-center rounded-full bg-hue-purple text-xs font-medium text-white">{tr("操", "Op")}</span>
        <ChevronDown className="h-3.5 w-3.5" />
      </button>
      {open && (
        <div className="absolute right-0 z-50 mt-1 w-44 rounded-md border border-line-strong bg-surface-raised py-1 shadow-xl">
          <div className="px-3 py-2 text-xs text-ink-faint">{tr("已登录（操作者）", "Signed in (operator)")}</div>
          <Link to="/settings" onClick={() => setOpen(false)} className="flex items-center gap-2 px-3 py-1.5 text-sm text-ink hover:bg-surface-hover">
            <SettingsIcon className="h-4 w-4 text-ink-muted" /> {tr("设置", "Settings")}
          </Link>
          <Link to="/audit" onClick={() => setOpen(false)} className="flex items-center gap-2 px-3 py-1.5 text-sm text-ink hover:bg-surface-hover">
            <ScrollText className="h-4 w-4 text-ink-muted" /> {tr("审计日志", "Audit log")}
          </Link>
          <button onClick={logOut} className="flex w-full items-center gap-2 px-3 py-1.5 text-sm text-ink hover:bg-surface-hover">
            <LogOut className="h-4 w-4 text-ink-muted" /> {tr("登出", "Sign out")}
          </button>
        </div>
      )}
    </div>
  );
}

