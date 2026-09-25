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
  markout: Capability;
  live: Capability;
  ops: Capability;
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

/** One horizon of markouts, in basis points; statistics absent when too few fills. */
export interface Horizon {
  seconds: number;
  samples: number;
  measured: boolean;
  mean_bps: number | null;
  median_bps: number | null;
  p10_bps: number | null;
  p90_bps: number | null;
  adverse_share: number | null;
}

export interface MarkoutComparison {
  ticks: string;
  baseline: { id: string; horizons: Horizon[] };
  candidate: { id: string; horizons: Horizon[] };
  contrast: { seconds: number; difference_bps: number | null }[];
}

// -- operations -----------------------------------------------------------

export interface Disk {
  mount: string;
  size: number | null;
  used: number | null;
  avail: number | null;
}

export interface HostHealth {
  load: number[];
  mem_total: number | null;
  mem_available: number | null;
  uptime_s: number | null;
  disks: Disk[];
  clock_synced: boolean | null;
  now_ms: number;
}

/** `systemctl show`, as the agent reports it. */
export interface UnitState {
  unit: string;
  manageable: boolean;
  ActiveState?: string;
  SubState?: string;
  Result?: string;
  NRestarts?: string;
  ExecMainStartTimestamp?: string;
  ExecMainPID?: string;
  ExecMainStatus?: string;
  UnitFileState?: string;
  error?: string;
}

/** The trader's own status, from its control port. */
export interface TraderStatus {
  pid: number;
  deployment: string;
  symbol: string;
  prefix: string;
  strategy: string;
  /** Decimal places of price and quantity; older traders do not say. */
  price_scale?: number;
  qty_scale?: number;
  now_ns: number;
  halted: boolean;
  halt_reason: string | null;
  journal_lost: string | null;
  resume_allowed: boolean;
  resting: number;
  ticks: number;
  last_tick: { exch_ns: number; local_ns: number; last: number } | null;
  positions: { side: string; amount: string }[];
  feed: {
    depth: number;
    trades: number;
    out_of_order: number;
    quiet: number;
    snapshots: number;
    resyncs: number;
    unreadable: number;
  };
  reconcile: { at_ns: number | null; agreed: boolean | null; mismatches: number; unread: number };
  waiting_on: Record<string, number>;
  counters: Record<string, number>;
}

export interface RestingOrder {
  local: number;
  client_id: string;
  closing: boolean;
  side: "BUY" | "SELL" | null;
  price_ticks: number | null;
  qty_lots: number | null;
}

export interface Alert {
  key: string;
  since_ms: number;
  message: string;
  silenced_until_ms?: number | null;
}

export interface AlertsView {
  active: Alert[];
  history: { at_ms: number; key: string; message: string; raised: boolean }[];
}

export interface ConfigFile {
  name: string;
  size: number;
  sha: string;
}

export interface ConfigDoc {
  name: string;
  content: string;
  sha: string;
  backups: { id: string; at_ms: number | null }[];
  backup_content: string | null;
}

export type Stage = "draft" | "backtested" | "observing" | "confirmed" | "live";

export interface StrategyView {
  instance: {
    id: string;
    name: string;
    config: string;
    stage: Stage;
    backtest_run: string | null;
    backtest_passed: boolean;
    config_sha: string | null;
    observing_since_ms: number | null;
    confirmed_by: string | null;
    history: { at_ms: number; from: Stage; to: Stage; actor: string; reason: string }[];
  };
  evidence: { observation_hours: number; observation_fills: number; required_hours: number; required_fills: number };
  config_sha_now: string | null;
  next: Stage | null;
  decision: { allowed: boolean; reason: string };
}

export interface Accounts {
  processes: Record<string, { log: string; fingerprint: string | null; line: string }>;
  same_account: boolean;
}

export interface LogFile {
  name: string;
  mtime: number;
  size: number;
}

export interface LogTail {
  name: string;
  size: number;
  truncated: boolean;
  lines: string[];
}

export interface AuditEntry {
  seq: number;
  at_ms: number;
  actor: string;
  op: string;
  reason: string;
  result: string;
  hash: string;
}

export interface AuditTrail {
  chain: { intact: boolean; problem?: string };
  entries: AuditEntry[];
}

export interface StagedRelease {
  id: string;
  verified: boolean;
  problem?: string;
  manifest?: { id: string; files: Record<string, string>; [k: string]: unknown };
}

export interface Releases {
  staged: StagedRelease[];
  installed: string[];
  current: string | null;
  previous: string | null;
  progress: {
    running: boolean;
    id: string;
    outcome: string | null;
    steps: { at_ms: number; step: string }[];
  };
}

export type OpsAction =
  | { action: "config_put"; name: string; content: string; base_sha: string }
  | { action: "config_rollback"; name: string; backup: string; base_sha: string }
  | { action: "strategy_create"; name: string; config: string }
  | { action: "strategy_backtest"; id: string; run: string; passed: boolean }
  | { action: "strategy_advance"; id: string }
  | { action: "alert_test" }
  | { action: "alert_silence"; key: string; minutes: number }
  | { action: "halt" }
  | { action: "shutdown" }
  | { action: "resume" }
  | { action: "rollback" }
  | { action: "deploy"; id: string }
  | { action: "unit"; unit: string; verb: "start" | "stop" | "restart" };

export interface LiveReconciliation {
  record_age_ms: number;
  reconciliation: {
    journal: string;
    believed: { symbol: string; read_at_ms: number; legs: [string, number, number][]; orders: string[] };
    venue: { symbol: string; read_at_ms: number; legs: [string, number, number][]; orders: string[] };
    differences: string[];
    verdict: "agree" | "disagree" | "cannot_tell";
    undecodable: number;
    hedged: boolean;
  };
}

// -- attribution ----------------------------------------------------------

export interface AttributionComponent {
  name: string;
  observed: boolean;
  amount: number | null;
  unavailable: string | null;
}

export interface AttributionReport {
  live_run?: string;
  model_run?: string;
  live_pnl: number;
  model_pnl: number;
  gap: number;
  components: AttributionComponent[];
  residual: number | null;
  residual_share: number | null;
  method: string;
  missing_inputs: string[];
  matched_fills: number;
  unmatched_fills: number;
}

/** The trader's shadow report, whose amounts arrive as decimal strings. */
function fromShadow(raw: Record<string, unknown>): AttributionReport {
  const n = (v: unknown) => (v === null || v === undefined ? null : Number(v));
  const components = (raw.components as Record<string, unknown>[]).map((c) => ({
    name: String(c.name),
    observed: Boolean(c.observed),
    amount: n(c.amount),
    unavailable: (c.unavailable as string | null) ?? null,
  }));
  return {
    live_pnl: Number(raw.live_pnl),
    model_pnl: Number(raw.model_pnl),
    gap: Number(raw.gap),
    components,
    residual: n(raw.residual),
    residual_share: n(raw.residual_share),
    method: "shadow",
    missing_inputs: components
      .filter((c) => c.amount === null && c.unavailable)
      .map((c) => `${c.name}：${c.unavailable}`),
    matched_fills: Number(raw.matched_fills ?? 0),
    unmatched_fills: Number(raw.unmatched_fills ?? 0),
  };
}

export interface RecordsPage {
  journal: string;
  total: number;
  records: { seq: number; kind: string; at: number | null; fields: Record<string, unknown> }[];
  next_before: number | null;
  price_scale: number;
  qty_scale: number;
  undecodable: number;
}

export interface JournalEntry {
  state: "read" | "unreadable";
  id: string;
  path: string;
  error?: string;
  belief?: {
    symbol: string | null;
    position_lots: number;
    entry_ticks: number;
    resting: string[];
    price_scale: number;
    qty_scale: number;
    adopted: boolean;
    hedged: boolean;
    undecodable: number;
    legs: [string, number, number][];
  };
}

export interface RuntimeSettings {
  listen: string;
  behind_tls: boolean;
  extra_hosts: string[];
  totp: boolean;
  allow_writes: boolean;
  runs_dir: string | null;
  journals_dir: string | null;
  ticks_dir: string | null;
  agent_socket: string | null;
  venue_record: string | null;
  session: { idle_minutes: number; absolute_hours: number };
  version: string;
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
  ticks: () => request<string[]>("/ticks"),
  markout: (baseline: string, candidate: string, ticks: string) =>
    request<MarkoutComparison>(
      `/runs/markout?baseline=${encodeURIComponent(baseline)}` +
        `&candidate=${encodeURIComponent(candidate)}&ticks=${encodeURIComponent(ticks)}`,
    ),
  session: () => request<SessionState>("/session"),
  login: (password: string, totp: string) =>
    post<{ authenticated: boolean }>("/session/login", { password, totp }),
  logout: () => post<{ authenticated: boolean }>("/session/logout", {}),
  setup: (token: string, password: string) => post<SetupDone>("/setup", { token, password }),
  host: () => request<HostHealth>("/ops/host"),
  units: () => request<UnitState[]>("/ops/units"),
  traderStatus: () => request<TraderStatus>("/ops/status"),
  orders: () => request<{ orders: RestingOrder[] }>("/ops/orders"),
  alerts: async () => (await request<AlertsView>("/ops/alerts")).active,
  alertsView: () => request<AlertsView>("/ops/alerts"),
  accounts: () => request<Accounts>("/ops/accounts"),
  configs: () => request<ConfigFile[]>("/ops/configs"),
  config: (name: string, backup?: string) =>
    request<ConfigDoc>(`/ops/config?name=${encodeURIComponent(name)}${backup ? `&backup=${encodeURIComponent(backup)}` : ""}`),
  strategies: () => request<StrategyView[]>("/ops/strategies"),
  logs: () => request<LogFile[]>("/ops/logs"),
  log: (name: string, lines: number, grep: string) =>
    request<LogTail>(
      `/ops/log?name=${encodeURIComponent(name)}&lines=${lines}&grep=${encodeURIComponent(grep)}`,
    ),
  audit: (lines = 200) => request<AuditTrail>(`/ops/audit?lines=${lines}`),
  releases: () => request<Releases>("/ops/releases"),
  act: (action: OpsAction, reason: string, stepUp: string) =>
    post<unknown>("/ops/action", { ...action, reason, step_up: stepUp || null }),
  liveLatest: () => request<LiveReconciliation>("/live/latest"),
  opsAttribution: async () => fromShadow(await request<Record<string, unknown>>("/ops/attribution")),
  attribution: (q: {
    live: string;
    model: string;
    price_scale: number;
    qty_scale: number;
    venue_fees: string;
    model_fees: string;
    venue_funding: string;
    model_funding: string;
  }) => {
    const p = new URLSearchParams({
      live: q.live,
      model: q.model,
      price_scale: String(q.price_scale),
      qty_scale: String(q.qty_scale),
    });
    for (const k of ["venue_fees", "model_fees", "venue_funding", "model_funding"] as const) {
      if (q[k] !== "") p.set(k, q[k]);
    }
    return request<AttributionReport>(`/attribution?${p}`);
  },
  journals: () => request<JournalEntry[]>("/journals"),
  records: (id: string, kinds: string[], limit: number, before?: number | null) =>
    request<RecordsPage>(
      `/journals/${encodeURIComponent(id)}/records?kinds=${kinds.join(",")}&limit=${limit}` +
        (before ? `&before=${before}` : ""),
    ),
  reconcile: (id: string, venueRecord: string) =>
    post<LiveReconciliation["reconciliation"]>(`/journals/${encodeURIComponent(id)}/reconcile`, {
      venue_record: venueRecord,
    }),
  settings: () => request<RuntimeSettings>("/runtime/settings"),
};
