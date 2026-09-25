import { createBrowserRouter } from "react-router-dom";

import { Placeholder } from "@/components/Placeholder";
import { Shell } from "@/components/Shell";
import { AuthGate } from "@/features/auth/AuthGate";
import { Login } from "@/features/auth/Login";
import { Setup } from "@/features/auth/Setup";

import { Compare } from "./Compare";
import { Ops } from "./Ops";
import { OpsAudit, OpsDeploy, OpsLogs, OpsOrders } from "./OpsPages";
import { Live } from "./Live";
import { Attribution } from "./Attribution";
import { Blackbox } from "./Blackbox";
import { Journal } from "./Journal";
import { Settings } from "./Settings";
import { Config } from "./Config";
import { Strategies } from "./Strategies";
import { Accounts, Alerts, DataQuality } from "./Watch";
import { Overview } from "./Overview";
import { RunDetail } from "./RunDetail";
import { Runs } from "./Runs";

/**
 * Every screen in the plan has an entry here from day one, so a design
 * that arrives for `/attribution` has somewhere to land and nothing has
 * to be renamed later. Screens not yet built name the milestone they
 * belong to rather than saying "coming soon", so a reader can tell
 * whether one is late or simply not due yet.
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
      { path: "runs", element: <Runs /> },
      { path: "runs/compare", element: <Compare /> },
      { path: "runs/:id", element: <RunDetail /> },
      { path: "attribution", element: <Attribution /> },
      { path: "live", element: <Live /> },
      { path: "ops", element: <Ops /> },
      { path: "ops/orders", element: <OpsOrders /> },
      { path: "ops/logs", element: <OpsLogs /> },
      { path: "ops/deploy", element: <OpsDeploy /> },
      { path: "ops/audit", element: <OpsAudit /> },
      { path: "blackbox", element: <Blackbox /> },
      { path: "journal", element: <Journal /> },
      {
        path: "sweeps",
        element: (
          <Placeholder title="参数扫描" milestone="M3" note="结果表一并给出 DSR / PBO 过拟合提示。" />
        ),
      },
      { path: "data", element: <DataQuality /> },
      { path: "strategies", element: <Strategies /> },
      { path: "config", element: <Config /> },
      { path: "alerts", element: <Alerts /> },
      { path: "accounts", element: <Accounts /> },
      { path: "settings", element: <Settings /> },
    ],
  },
]);
