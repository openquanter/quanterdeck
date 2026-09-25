import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { api, type CapabilityName } from "@/api/client";
import { ModeToggle } from "@/components/States";

/**
 * Navigation is rendered from the deck's capabilities, not from a fixed
 * list. A section the deck cannot back produces no link, so nothing in
 * the interface leads somewhere that will fail — and the reason it is
 * missing is on the overview rather than nowhere.
 */
const SECTIONS: { to: string; label: string; capability: CapabilityName | null }[] = [
  { to: "/", label: "总览", capability: null },
  { to: "/ops", label: "运维", capability: "ops" },
  { to: "/ops/orders", label: "挂单", capability: "ops" },
  { to: "/live", label: "实盘对账", capability: "live" },
  { to: "/attribution", label: "归因", capability: "attribution" },
  { to: "/journal", label: "Journal 回放", capability: "live" },
  { to: "/ops/logs", label: "日志", capability: "ops" },
  { to: "/ops/deploy", label: "部署", capability: "ops" },
  { to: "/config", label: "配置", capability: "ops" },
  { to: "/strategies", label: "策略与门控", capability: "ops" },
  { to: "/alerts", label: "告警", capability: "ops" },
  { to: "/accounts", label: "账户", capability: "ops" },
  { to: "/ops/audit", label: "审计", capability: "ops" },
  { to: "/runs", label: "运行记录", capability: "runs" },

  { to: "/sweeps", label: "参数扫描", capability: "runs" },
  { to: "/data", label: "数据质量", capability: "live" },
  { to: "/settings", label: "设置", capability: null },
];

export function Shell() {
  const { data: caps } = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });

  return (
    <div className="flex min-h-screen">
      <nav className="w-52 shrink-0 border-r border-line bg-surface p-4">
        <div className="mb-6 px-2 font-mono text-sm tracking-tight text-ink">
          quanterdeck
          <div className="mt-1 text-xs text-ink-muted">
            {/* From the capability, not a literal: this said "只读" whether
                or not writes were on. */}
            {caps ? `v${caps.version} · ${caps.writes.available ? "可写入" : "只读"}` : " "}
          </div>
        </div>
        <ul className="space-y-1">
          {SECTIONS.filter(
            (s) => s.capability === null || caps?.[s.capability]?.available,
          ).map((section) => (
            <li key={section.to}>
              <NavLink
                to={section.to}
                end={section.to === "/"}
                className={({ isActive }) =>
                  [
                    "block rounded px-2 py-1.5 text-sm",
                    isActive
                      ? "bg-surface-raised text-ink"
                      : "text-ink-muted hover:text-ink",
                  ].join(" ")
                }
              >
                {section.label}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>
      <main className="flex-1">
        <SessionBar />
        <div className="p-6">
          <Outlet />
        </div>
      </main>
    </div>
  );
}

/** Top right: whose session this is, and the way out (UI-BRIEF §5.3). */
function SessionBar() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  async function logOut() {
    await api.logout().catch(() => undefined);
    await queryClient.invalidateQueries();
    navigate("/login", { replace: true });
  }
  return (
    <div className="flex items-center justify-end gap-3 border-b border-line px-6 py-2 text-xs text-ink-muted">
      <ModeToggle />
      <span>已登录</span>
      <button type="button" onClick={logOut} className="rounded border border-line px-2 py-1 hover:text-ink">
        登出
      </button>
    </div>
  );
}
