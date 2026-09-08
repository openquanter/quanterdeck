import { createBrowserRouter } from "react-router-dom";

import { Placeholder } from "@/components/Placeholder";
import { Shell } from "@/components/Shell";

import { Overview } from "./Overview";
import { Services } from "./Services";

/**
 * The route table is the contract with the design work: every screen in
 * the plan has an entry here from day one, so a design that arrives for
 * `/backtests/:id` has somewhere to land and nothing has to be renamed
 * later. Screens not yet built render a placeholder naming the
 * milestone they belong to.
 */
export const router = createBrowserRouter([
  {
    path: "/",
    element: <Shell />,
    children: [
      { index: true, element: <Overview /> },
      { path: "services", element: <Services /> },
      {
        path: "live",
        element: <Placeholder title="实盘" milestone="M1" />,
      },
      {
        path: "live/logs",
        element: <Placeholder title="日志" milestone="M1" />,
      },
      {
        path: "strategies",
        element: <Placeholder title="策略列表" milestone="M4" />,
      },
      {
        path: "strategies/:id",
        element: <Placeholder title="策略详情" milestone="M4" />,
      },
      {
        path: "strategies/:id/edit",
        element: <Placeholder title="策略编辑器" milestone="M4" />,
      },
      {
        path: "backtests",
        element: <Placeholder title="回测中心" milestone="M2" />,
      },
      {
        path: "backtests/:id",
        element: <Placeholder title="回测结果" milestone="M2" />,
      },
      {
        path: "backtests/compare",
        element: <Placeholder title="结果对比" milestone="M2" />,
      },
      {
        path: "sweeps",
        element: (
          <Placeholder
            title="参数扫描"
            milestone="M2"
            note="结果表会一并给出 DSR / PBO 过拟合提示。"
          />
        ),
      },
      {
        path: "config",
        element: <Placeholder title="配置中心" milestone="M1" />,
      },
      {
        path: "config/history",
        element: <Placeholder title="变更历史" milestone="M1" />,
      },
      {
        path: "exchanges",
        element: (
          <Placeholder
            title="交易所与密钥"
            milestone="M3"
            note="密钥加密存放在本机，不出现在任何 API 响应中。"
          />
        ),
      },
      {
        path: "alerts",
        element: <Placeholder title="告警" milestone="M5" />,
      },
      {
        path: "settings",
        element: <Placeholder title="系统设置" milestone="M0" />,
      },
    ],
  },
  {
    path: "/setup",
    element: <Placeholder title="首次运行向导" milestone="M3" />,
  },
]);
