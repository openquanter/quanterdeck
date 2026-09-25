import { createBrowserRouter } from "react-router-dom";

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
import { Sweeps } from "./Sweeps";
import { Accounts, Alerts, DataQuality } from "./Watch";
import { Overview } from "./Overview";
import { RunDetail } from "./RunDetail";
import { Runs } from "./Runs";

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
      { path: "sweeps", element: <Sweeps /> },
      { path: "data", element: <DataQuality /> },
      { path: "strategies", element: <Strategies /> },
      { path: "config", element: <Config /> },
      { path: "alerts", element: <Alerts /> },
      { path: "accounts", element: <Accounts /> },
      { path: "settings", element: <Settings /> },
    ],
  },
]);
