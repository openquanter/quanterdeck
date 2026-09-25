import { Navigate, createBrowserRouter } from "react-router-dom";

import { Shell } from "@/components/Shell";
import { AuthGate } from "@/features/auth/AuthGate";
import { Login } from "@/features/auth/Login";
import { Setup } from "@/features/auth/Setup";

import { Accounts, Alerts } from "./Watch";
import { Blackbox } from "./Blackbox";
import { Compare } from "./Compare";
import { Config } from "./Config";
import { Journal } from "./Journal";
import { Ops } from "./Ops";
import { OpsAudit, OpsDeploy, OpsLogs } from "./OpsPages";
import { Overview } from "./Overview";
import { Reconcile } from "./Reconcile";
import { RunDetail } from "./RunDetail";
import { Runs } from "./Runs";
import { Settings } from "./Settings";
import { Strategies } from "./Strategies";
import { Sweeps } from "./Sweeps";
import { Trading } from "./Trading";

/**
 * Every screen in the plan, one entry each. The navigation is drawn from
 * the deck's capabilities (Shell), so a screen the deck cannot back has
 * no link even though its route exists.
 */
export const router = createBrowserRouter([
  { path: "/login", element: <Login /> },
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
      { path: "live", element: <Trading /> },
      { path: "reconcile", element: <Reconcile /> },
      // 诊断
      { path: "alerts", element: <Alerts /> },
      { path: "blackbox", element: <Blackbox /> },
      { path: "logs", element: <OpsLogs /> },
      { path: "journal", element: <Journal /> },
      // 变更
      { path: "strategies", element: <Strategies /> },
      { path: "config", element: <Config /> },
      { path: "deploy", element: <OpsDeploy /> },
      // 研究
      { path: "runs", element: <Runs /> },
      { path: "runs/compare", element: <Compare /> },
      { path: "runs/:id", element: <RunDetail /> },
      { path: "sweeps", element: <Sweeps /> },
      // 系统
      { path: "host", element: <Ops /> },
      { path: "accounts", element: <Accounts /> },
      { path: "audit", element: <OpsAudit /> },
      { path: "settings", element: <Settings /> },
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
