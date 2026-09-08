/**
 * The one place that talks to the deck.
 *
 * Hand-written for now. Once the route surface settles it is generated
 * from the server's OpenAPI document by `scripts/gen-client.sh`, so the
 * types the UI compiles against are the types the server actually
 * serves — a drift that costs an afternoon to find is not worth the
 * convenience of writing them twice.
 */

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly detail: unknown,
  ) {
    super(typeof detail === "string" ? detail : JSON.stringify(detail));
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api/v1${path}`, {
    headers: { "Content-Type": "application/json" },
    ...init,
  });
  if (!response.ok) {
    const body = await response.json().catch(() => null);
    throw new ApiError(response.status, body?.detail ?? response.statusText);
  }
  return response.json() as Promise<T>;
}

export interface Capabilities {
  kind: string;
  version: string;
  services: boolean;
  config_read: boolean;
  config_write: boolean;
  strategy_schema: boolean;
  strategy_edit: boolean;
  backtest: boolean;
  sweep: boolean;
  live_state: boolean;
  log_stream: boolean;
  notes: Record<string, string>;
}

export type ServiceStatus = "running" | "stopped" | "unknown";

export interface Service {
  name: string;
  status: ServiceStatus;
  pid: number | null;
  actions: string[];
  detail: string;
}

export interface StrategyClass {
  class_name: string;
  module: string;
  author: string;
  parameters: {
    name: string;
    type: string;
    default: unknown;
    description: string;
  }[];
  variables: string[];
}

export const api = {
  capabilities: () => request<Capabilities>("/runtime/capabilities"),
  services: () => request<Service[]>("/services"),
  strategyClasses: () => request<StrategyClass[]>("/strategies/classes"),

  /**
   * Act on a service. `confirm` must be the service's own name for the
   * actions that carry a consequence; the server refuses otherwise and
   * returns what would have happened, which the dialog shows verbatim.
   */
  serviceAction: (name: string, action: string, confirm = "") =>
    request<{ ok: boolean; stdout: string; stderr: string }>(
      `/services/${name}/${action}`,
      { method: "POST", body: JSON.stringify({ confirm }) },
    ),
};
