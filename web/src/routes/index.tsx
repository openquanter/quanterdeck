import { lazy, type ComponentType } from "react";
import { Navigate, createBrowserRouter } from "react-router-dom";

import { Shell } from "@/components/Shell";
import { AuthGate } from "@/features/auth/AuthGate";
import { Enrol } from "@/features/auth/Enrol";
import { Login } from "@/features/auth/Login";
import { Setup } from "@/features/auth/Setup";

import { Overview } from "./Overview";

/**
 * Each screen is its own chunk, loaded when first visited: the overview
 * opens without first downloading the chart library every research
 * screen needs.
 */
function page<K extends string>(load: () => Promise<Record<K, ComponentType>>, name: K) {
  const Lazy = lazy<ComponentType>(() => load().then((m) => ({ default: m[name] as ComponentType })));
  return <Lazy />;
}

/**
 * Every screen in the plan, one entry each. The navigation is drawn from
 * the deck's capabilities (Shell), so a screen the deck cannot back has
 * no link even though its route exists.
 */
export const router = createBrowserRouter([
  { path: "/login", element: <Login /> },
  { path: "/enrol", element: <Enrol /> },
  { path: "/setup", element: <Setup /> },
  {
    path: "/",
    element: (
      <AuthGate>
        <Shell />
      </AuthGate>
    ),
    children: [
      { index: true, element: <Overview /> },
      // 交易
      { path: "live", element: page(() => import("./Trading"), "Trading") },
      { path: "traders", element: page(() => import("./Traders"), "Traders") },
      { path: "reconcile", element: page(() => import("./Reconcile"), "Reconcile") },
      // 诊断
      { path: "alerts", element: page(() => import("./Alerts"), "Alerts") },
      { path: "blackbox", element: page(() => import("./Blackbox"), "Blackbox") },
      { path: "reports", element: page(() => import("./Reports"), "Reports") },
      { path: "logs", element: page(() => import("./Logs"), "Logs") },
      { path: "journal", element: page(() => import("./Journal"), "Journal") },
      // 变更
      { path: "strategies", element: page(() => import("./Strategies"), "Strategies") },
      { path: "config", element: page(() => import("./Config"), "Config") },
      { path: "deploy", element: page(() => import("./Deploy"), "Deploy") },
      // 研究
      { path: "runs", element: page(() => import("./Runs"), "Runs") },
      { path: "runs/compare", element: page(() => import("./Compare"), "Compare") },
      { path: "runs/:id", element: page(() => import("./RunDetail"), "RunDetail") },
      { path: "sweeps", element: page(() => import("./Sweeps"), "Sweeps") },
      // 系统
      { path: "host", element: page(() => import("./Host"), "Host") },
      { path: "accounts", element: page(() => import("./Accounts"), "Accounts") },
      { path: "audit", element: page(() => import("./Audit"), "Audit") },
      { path: "settings", element: page(() => import("./Settings"), "Settings") },
      // Where things were before docs/UI-V4, so bookmarks still land.
      { path: "ops", element: <Navigate to="/host" replace /> },
      { path: "ops/orders", element: <Navigate to="/live?tab=positions" replace /> },
      { path: "ops/logs", element: <Navigate to="/logs" replace /> },
      { path: "ops/deploy", element: <Navigate to="/deploy" replace /> },
      { path: "ops/audit", element: <Navigate to="/audit" replace /> },
      { path: "attribution", element: <Navigate to="/reconcile?tab=attribution" replace /> },
      { path: "data", element: <Navigate to="/live?tab=market" replace /> },
      { path: "*", element: <Navigate to="/" replace /> },
    ],
  },
]);
