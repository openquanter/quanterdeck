/**
 * The one place that talks to the deck.
 *
 * Hand-written while the surface is small. Once it settles it comes from
 * the server's own schema, so the types the interface compiles against
 * are the types the server serves.
 */

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly detail: string,
  ) {
    super(detail);
  }
}

/**
 * Raised when an authenticated request comes back 401: the session ended
 * under a page the operator was reading. The auth gate answers it with a
 * sign-in dialog over that page rather than a navigation away from it.
 */
export const UNAUTHENTICATED = "oq:unauthenticated";

async function read<T>(response: Response, path: string): Promise<T> {
  if (!response.ok) {
    const body = await response.json().catch(() => null);
    if (response.status === 401 && path !== "/session/login" && path !== "/setup") {
      window.dispatchEvent(new Event(UNAUTHENTICATED));
    }
    throw new ApiError(response.status, body?.detail ?? response.statusText);
  }
  return response.json() as Promise<T>;
}

async function request<T>(path: string): Promise<T> {
  return read<T>(await fetch(`/api/v1${path}`), path);
}

/** A write. The browser sets `Origin`, which the deck checks on every one. */
async function post<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`/api/v1${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  return read<T>(response, path);
}

export interface SessionState {
  authenticated: boolean;
  /** True until the first password is set; the interface goes to setup. */
  setup_required: boolean;
  /** Whether the login form needs a six-digit code field. */
  totp_required: boolean;
}

export interface SetupDone {
  password_hash: string;
  totp_secret: string;
  next_steps: string[];
}

/** A capability that is off carries the sentence explaining it. */
export interface Capability {
  available: boolean;
  reason: string;
}

export interface Capabilities {
  version: string;
  runs: Capability;
  attribution: Capability;
  live: Capability;
  writes: Capability;
}

export type CapabilityName = Exclude<keyof Capabilities, "version">;

export interface Identity {
  code_commit: string;
  data_hash: string;
  config_hash: string;
  label: string;
}

export interface RunSummary {
  state: "read";
  id: string;
  path: string;
  identity: Identity;
  pnl: number;
  fills: number;
}

export interface UnreadableRun {
  state: "unreadable";
  id: string;
  path: string;
  error: string;
}

export type RunEntry = RunSummary | UnreadableRun;

export interface Listing {
  entries: RunEntry[];
  /** `null` when no total means anything: a run would not read, or the
   * runs are of different kinds. */
  total_pnl: number | null;
}

export interface Fill {
  ts: number;
  symbol: string;
  side: string;
  price_ticks: number;
  qty_lots: number;
  tag: string | null;
}

export type RunDetail = Omit<RunSummary, "state"> & { fills: Fill[] };

export interface Verdict {
  status: "comparable" | "code_changed" | "invalidated";
  conclusive: boolean;
  changed: string[];
}

export interface Comparison {
  baseline: string;
  candidate: string;
  verdict: Verdict;
  differences: number;
  first_divergence: number | null;
  matched_prefix: number;
  fill_counts: [number, number];
  pnl: [number, number];
  pnl_relative_error: number | null;
  passes: boolean;
}

export const api = {
  capabilities: () => request<Capabilities>("/runtime/capabilities"),
  runs: () => request<Listing>("/runs"),
  run: (id: string) => request<RunDetail>(`/runs/${encodeURIComponent(id)}`),
  compare: (baseline: string, candidate: string, tolerance = 0) =>
    request<Comparison>(
      `/runs/compare?baseline=${encodeURIComponent(baseline)}` +
        `&candidate=${encodeURIComponent(candidate)}&tolerance=${tolerance}`,
    ),
  session: () => request<SessionState>("/session"),
  login: (password: string, totp: string) =>
    post<{ authenticated: boolean }>("/session/login", { password, totp }),
  logout: () => post<{ authenticated: boolean }>("/session/logout", {}),
  setup: (token: string, password: string) => post<SetupDone>("/setup", { token, password }),
};
