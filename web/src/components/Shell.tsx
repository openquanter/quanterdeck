import { NavLink, Outlet } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";

import { api } from "@/api/client";

/**
 * Navigation is rendered from the runtime's capabilities, not from a
 * fixed list. A runtime that cannot back a section produces no link to
 * it, so nothing in the UI leads somewhere that will fail.
 */
const SECTIONS = [
  { to: "/", label: "总览", capability: null },
  { to: "/live", label: "实盘", capability: "live_state" },
  { to: "/services", label: "服务", capability: "services" },
  { to: "/strategies", label: "策略", capability: "strategy_schema" },
  { to: "/backtests", label: "回测", capability: "backtest" },
  { to: "/sweeps", label: "参数扫描", capability: "sweep" },
  { to: "/config", label: "配置", capability: "config_read" },
  { to: "/exchanges", label: "交易所", capability: null },
  { to: "/settings", label: "设置", capability: null },
] as const;

export function Shell() {
  const { data: caps } = useQuery({
    queryKey: ["capabilities"],
    queryFn: api.capabilities,
  });

  return (
    <div className="flex min-h-screen">
      <nav className="w-52 shrink-0 border-r border-line bg-surface p-4">
        <div className="mb-6 px-2 font-mono text-sm tracking-tight text-ink">
          quanterdeck
          {caps && (
            <div className="mt-1 text-xs text-ink-muted">
              {caps.kind} · {caps.version}
            </div>
          )}
        </div>
        <ul className="space-y-1">
          {SECTIONS.map((section) => {
            const enabled =
              section.capability === null ||
              caps?.[section.capability as keyof typeof caps] === true;
            if (!enabled) return null;
            return (
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
            );
          })}
        </ul>
      </nav>
      <main className="flex-1 p-6">
        <Outlet />
      </main>
    </div>
  );
}
