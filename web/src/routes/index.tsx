import { createBrowserRouter } from "react-router-dom";

import { Placeholder } from "@/components/Placeholder";
import { Shell } from "@/components/Shell";
import { AuthGate } from "@/features/auth/AuthGate";
import { Login } from "@/features/auth/Login";
import { Setup } from "@/features/auth/Setup";

import { Compare } from "./Compare";
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
      {
        path: "attribution",
        element: (
          <Placeholder
            title="归因"
            milestone="M2"
            note="实盘减模型的差额分解，以及不可分解的残差。残差在分解不完整时是「未知」而不是零。"
          />
        ),
      },
      {
        path: "live",
        element: (
          <Placeholder
            title="实盘对账"
            milestone="M1"
            note="进程以为持有的（journal）与交易所实际持有的（venue）之间的差别；仍为 stale 的订单是边沿触发的告警。"
          />
        ),
      },
      { path: "journal", element: <Placeholder title="Journal 回放" milestone="M1" /> },
      {
        path: "sweeps",
        element: (
          <Placeholder title="参数扫描" milestone="M3" note="结果表一并给出 DSR / PBO 过拟合提示。" />
        ),
      },
      {
        path: "data",
        element: (
          <Placeholder title="数据质量" milestone="M3" note="capture → ingest → 特征化；book-check 与 trade-check 的 break。" />
        ),
      },
      { path: "strategies", element: <Placeholder title="策略" milestone="M4" /> },
      { path: "settings", element: <Placeholder title="设置" milestone="M1" /> },
    ],
  },
]);
