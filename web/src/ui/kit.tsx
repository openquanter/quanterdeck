import { useEffect, useState, type ReactNode } from "react";
import { useSearchParams } from "react-router-dom";
import { Info, Loader2, RefreshCw, X } from "lucide-react";

import { GLOSSARY } from "@/components/States";

/**
 * The console's building blocks. Pages assemble these and do not style
 * their own cards, tables or buttons: one look, defined once, is what
 * makes fifteen screens read as one tool.
 */

export function cx(...parts: (string | false | null | undefined)[]) {
  return parts.filter(Boolean).join(" ");
}

export type Tone = "good" | "warn" | "bad" | "neutral" | "accent";

const TONE_TEXT: Record<Tone, string> = {
  good: "text-good",
  warn: "text-warn",
  bad: "text-bad",
  neutral: "text-ink",
  accent: "text-accent",
};
const TONE_DOT: Record<Tone, string> = {
  good: "bg-good",
  warn: "bg-warn",
  bad: "bg-bad",
  neutral: "bg-ink-faint",
  accent: "bg-accent",
};
const TONE_SOFT: Record<Tone, string> = {
  good: "bg-good/12 text-good ring-good/25",
  warn: "bg-warn/12 text-warn ring-warn/25",
  bad: "bg-bad/12 text-bad ring-bad/30",
  neutral: "bg-surface-raised text-ink-muted ring-line-strong",
  accent: "bg-accent/12 text-accent ring-accent/25",
};

/** A page's title, what it is for, and its actions. */
export function PageHeader({
  title,
  description,
  actions,
  meta,
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  meta?: ReactNode;
}) {
  return (
    <div className="mb-6 flex flex-wrap items-start gap-4">
      <div className="min-w-0 flex-1">
        <h1 className="text-xl font-semibold tracking-tight text-ink">{title}</h1>
        {description && <p className="mt-1 text-sm text-ink-muted">{description}</p>}
      </div>
      {(meta || actions) && (
        <div className="flex flex-wrap items-center gap-2">
          {meta}
          {actions}
        </div>
      )}
    </div>
  );
}

/** A titled panel. */
export function Card({
  title,
  icon,
  extra,
  children,
  className,
  bodyClassName,
  tone,
}: {
  title?: ReactNode;
  icon?: ReactNode;
  extra?: ReactNode;
  children: ReactNode;
  className?: string;
  bodyClassName?: string;
  tone?: Tone;
}) {
  return (
    <section
      className={cx(
        "rounded-[var(--radius-card)] border bg-surface",
        tone === "bad" ? "border-bad/40" : tone === "warn" ? "border-warn/40" : "border-line",
        className,
      )}
    >
      {(title || extra) && (
        <header className="flex items-center gap-2 border-b border-line px-4 py-3">
          {icon && <span className="text-ink-muted">{icon}</span>}
          <h2 className="text-sm font-medium text-ink">{title}</h2>
          <div className="ml-auto flex items-center gap-2 text-xs text-ink-muted">{extra}</div>
        </header>
      )}
      <div className={cx("p-4", bodyClassName)}>{children}</div>
    </section>
  );
}

/** One number that matters, with what it means. */
export function Stat({
  label,
  value,
  sub,
  tone,
  help,
  icon,
}: {
  label: ReactNode;
  value: ReactNode;
  sub?: ReactNode;
  tone?: Tone;
  help?: string;
  icon?: ReactNode;
}) {
  return (
    <div className="rounded-[var(--radius-card)] border border-line bg-surface px-4 py-3.5">
      <div className="flex items-center gap-1.5 text-xs text-ink-muted">
        {icon}
        <span>{label}</span>
        {help && <Help term={help} />}
      </div>
      <div className={cx("mt-1.5 truncate text-2xl font-semibold tracking-tight", TONE_TEXT[tone ?? "neutral"])}>{value}</div>
      {sub && <div className="mt-1 truncate text-xs text-ink-muted">{sub}</div>}
    </div>
  );
}

export function StatusDot({ tone, pulse }: { tone: Tone; pulse?: boolean }) {
  return (
    <span className="relative inline-flex h-2 w-2 shrink-0">
      {pulse && <span className={cx("absolute inline-flex h-full w-full animate-ping rounded-full opacity-50", TONE_DOT[tone])} />}
      <span className={cx("relative inline-flex h-2 w-2 rounded-full", TONE_DOT[tone])} />
    </span>
  );
}

export function Badge({ tone = "neutral", children, dot }: { tone?: Tone; children: ReactNode; dot?: boolean }) {
  return (
    <span className={cx("inline-flex items-center gap-1.5 whitespace-nowrap rounded-full px-2 py-0.5 text-xs ring-1 ring-inset", TONE_SOFT[tone])}>
      {dot && <StatusDot tone={tone} />}
      {children}
    </span>
  );
}

type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";
const BUTTON: Record<ButtonVariant, string> = {
  primary: "bg-accent text-white hover:bg-accent/90",
  secondary: "border border-line-strong bg-surface-raised text-ink hover:bg-surface-hover",
  danger: "border border-bad/40 bg-bad/10 text-bad hover:bg-bad/20",
  ghost: "text-ink-muted hover:bg-surface-hover hover:text-ink",
};

export function Button({
  children,
  onClick,
  variant = "secondary",
  icon,
  disabled,
  size = "md",
  title,
  type = "button",
}: {
  children?: ReactNode;
  onClick?: () => void;
  variant?: ButtonVariant;
  icon?: ReactNode;
  disabled?: boolean;
  size?: "sm" | "md";
  title?: string;
  type?: "button" | "submit";
}) {
  return (
    <button
      type={type}
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={cx(
        "inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40",
        size === "sm" ? "h-7 px-2.5 text-xs" : "h-8 px-3 text-sm",
        BUTTON[variant],
      )}
    >
      {icon}
      {children}
    </button>
  );
}

/** A term's plain-language meaning, on hover. Replaces the novice mode. */
export function Help({ term, text }: { term?: string; text?: string }) {
  const body = text ?? (term ? GLOSSARY[term] : undefined);
  if (!body) return null;
  return (
    <span className="group relative inline-flex">
      <Info className="h-3.5 w-3.5 cursor-help text-ink-faint hover:text-ink-muted" />
      <span className="pointer-events-none absolute left-1/2 top-full z-40 mt-2 hidden w-72 -translate-x-1/2 rounded-md border border-line-strong bg-surface-raised px-3 py-2 text-xs font-normal leading-relaxed text-ink shadow-xl group-hover:block">
        {body}
      </span>
    </span>
  );
}

/** Tabs whose selection lives in the URL, so a link lands on the tab. */
export function TabBar<K extends string>({
  tabs,
  param = "tab",
}: {
  tabs: { key: K; label: ReactNode; badge?: ReactNode }[];
  param?: string;
}) {
  const [params, setParams] = useSearchParams();
  const current = tabs.find((t) => t.key === params.get(param))?.key ?? tabs[0].key;
  return (
    <div className="mb-4 flex gap-1 border-b border-line">
      {tabs.map((t) => (
        <button
          key={t.key}
          onClick={() => {
            const next = new URLSearchParams(params);
            next.set(param, t.key);
            setParams(next, { replace: true });
          }}
          className={cx(
            "-mb-px inline-flex items-center gap-2 border-b-2 px-3 py-2 text-sm transition-colors",
            t.key === current ? "border-accent text-ink" : "border-transparent text-ink-muted hover:text-ink",
          )}
        >
          {t.label}
          {t.badge}
        </button>
      ))}
    </div>
  );
}

/** Which tab is selected; pair with `TabBar`. */
export function useTab<K extends string>(keys: readonly K[], param = "tab"): K {
  const [params] = useSearchParams();
  const v = params.get(param);
  return (keys as readonly string[]).includes(v ?? "") ? (v as K) : keys[0];
}

/** A plain data table with the console's look. */
export function Table({
  head,
  children,
  className,
  dense,
}: {
  head: ReactNode[];
  children: ReactNode;
  className?: string;
  dense?: boolean;
}) {
  return (
    <div className={cx("overflow-x-auto", className)}>
      <table className="w-full text-[13px]">
        <thead>
          <tr className="border-b border-line text-left text-xs text-ink-muted">
            {head.map((h, i) => (
              <th key={i} className={cx("whitespace-nowrap px-3 font-medium", dense ? "py-1.5" : "py-2")}>
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="[&>tr]:border-b [&>tr]:border-line/60 [&>tr:last-child]:border-0 [&>tr:hover]:bg-surface-hover/50 [&_td]:px-3 [&_td]:py-2">
          {children}
        </tbody>
      </table>
    </div>
  );
}

/** Label/value pairs. */
export function KV({ items, cols = 1 }: { items: [ReactNode, ReactNode][]; cols?: 1 | 2 | 3 }) {
  return (
    <dl className={cx("grid gap-x-6 gap-y-2.5 text-sm", cols === 2 && "sm:grid-cols-2", cols === 3 && "sm:grid-cols-3")}>
      {items.map(([k, v], i) => (
        <div key={i} className="flex min-w-0 items-baseline justify-between gap-3">
          <dt className="shrink-0 text-ink-muted">{k}</dt>
          <dd className="min-w-0 truncate text-right text-ink">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

/** A panel sliding in from the right, for detail that should not leave the page. */
export function Drawer({ title, onClose, children, width = "max-w-2xl" }: { title: ReactNode; onClose: () => void; children: ReactNode; width?: string }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/50" onClick={onClose}>
      <div className={cx("flex h-full w-full flex-col border-l border-line bg-surface shadow-2xl", width)} onClick={(e) => e.stopPropagation()}>
        <header className="flex items-center border-b border-line px-5 py-3.5">
          <h2 className="text-base font-medium text-ink">{title}</h2>
          <button className="ml-auto rounded p-1 text-ink-muted hover:bg-surface-hover hover:text-ink" onClick={onClose} aria-label="关闭">
            <X className="h-4 w-4" />
          </button>
        </header>
        <div className="flex-1 overflow-auto p-5">{children}</div>
      </div>
    </div>
  );
}

/** A small segmented choice, for ranges and filters. */
export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: ReactNode }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="inline-flex rounded-md border border-line bg-ground p-0.5">
      {options.map((o) => (
        <button
          key={String(o.value)}
          onClick={() => onChange(o.value)}
          className={cx(
            "rounded px-2.5 py-1 text-xs transition-colors",
            o.value === value ? "bg-surface-raised text-ink shadow-sm" : "text-ink-muted hover:text-ink",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** When the data on screen was fetched, and whether that is too long ago. */
export function Freshness({ at, fetching, staleAfterS = 60, onRefresh }: { at: number; fetching?: boolean; staleAfterS?: number; onRefresh?: () => void }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  if (!at) return null;
  const age = Math.max(0, Math.round((now - at) / 1000));
  return (
    <button
      onClick={onRefresh}
      title="刷新"
      className={cx("inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs hover:bg-surface-hover", age > staleAfterS ? "text-warn" : "text-ink-faint")}
    >
      {fetching ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
      {age < 2 ? "刚刚更新" : `${fmtDuration(age)}前更新`}
    </button>
  );
}

// -- formatting --------------------------------------------------------------

export function fmtDuration(seconds: number): string {
  const s = Math.round(Math.abs(seconds));
  if (s < 60) return `${s} 秒`;
  if (s < 3600) return `${Math.floor(s / 60)} 分钟`;
  if (s < 86400) return `${Math.floor(s / 3600)} 小时${Math.floor((s % 3600) / 60) ? ` ${Math.floor((s % 3600) / 60)} 分` : ""}`;
  return `${Math.floor(s / 86400)} 天${Math.floor((s % 86400) / 3600) ? ` ${Math.floor((s % 86400) / 3600)} 小时` : ""}`;
}

export function fmtTime(ms: number, withDate = true): string {
  return new Date(ms).toLocaleString("zh-CN", withDate ? { hour12: false } : { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/** "3 分钟前", with the exact time on hover. */
export function Ago({ ms }: { ms: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 5000);
    return () => clearInterval(t);
  }, []);
  const s = (now - ms) / 1000;
  return <span title={fmtTime(ms)}>{s < 5 ? "刚刚" : `${fmtDuration(s)}前`}</span>;
}

export function fmtBytes(b: number | null | undefined): string {
  if (b == null) return "—";
  if (b >= 2 ** 30) return `${(b / 2 ** 30).toFixed(1)} GiB`;
  if (b >= 2 ** 20) return `${(b / 2 ** 20).toFixed(1)} MiB`;
  if (b >= 2 ** 10) return `${(b / 2 ** 10).toFixed(1)} KiB`;
  return `${b} B`;
}

/** A signed money amount, coloured by sign only when asked. */
export function Money({ value, signed }: { value: string | number | null | undefined; signed?: boolean }) {
  if (value == null || value === "") return <span className="text-ink-faint">—</span>;
  const n = Number(value);
  const text = Number.isFinite(n) ? n.toLocaleString("zh-CN", { maximumFractionDigits: 4 }) : String(value);
  if (!signed || !Number.isFinite(n) || n === 0) return <span>{text}</span>;
  return <span className={n > 0 ? "text-good" : "text-bad"}>{n > 0 ? `+${text}` : text}</span>;
}
