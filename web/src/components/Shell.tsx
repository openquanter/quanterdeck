import { NavLink, Outlet } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";

import { api, type CapabilityName } from "@/api/client";

/**
 * Navigation is rendered from the deck's capabilities, not from a fixed
 * list. A section the deck cannot back produces no link, so nothing in
 * the interface leads somewhere that will fail — and the reason it is
 * missing is on the overview rather than nowhere.
 */
const SECTIONS: { to: string; label: string; capability: CapabilityName | null }[] = [
  { to: "/", label: "总览", capability: null },
  { to: "/runs", label: "运行记录", capability: "runs" },
  { to: "/attribution", label: "归因", capability: "attribution" },
  { to: "/live", label: "实盘对账", capability: "live" },
  { to: "/journal", label: "Journal", capability: "live" },
  { to: "/sweeps", label: "参数扫描", capability: "runs" },
  { to: "/data", label: "数据质量", capability: null },
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
      <main className="flex-1 p-6">
        <Outlet />
      </main>
    </div>
  );
}
