import { useEffect, useState, type ReactNode } from "react";
import { useSearchParams } from "react-router-dom";
import { Info, Loader2, RefreshCw, X } from "lucide-react";

import { glossary } from "@/components/States";
import { intlLocale, tr } from "@/i18n";

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

/** Colours for icons and chart series: they tell things apart and mean nothing. */
export type Hue = "blue" | "green" | "yellow" | "red" | "purple" | "teal" | "pink" | "orange";

const HUE: Record<Hue, string> = {
  blue: "bg-hue-blue/12 text-hue-blue",
  green: "bg-hue-green/12 text-hue-green",
  yellow: "bg-hue-yellow/15 text-hue-yellow",
  red: "bg-hue-red/12 text-hue-red",
  purple: "bg-hue-purple/12 text-hue-purple",
  teal: "bg-hue-teal/12 text-hue-teal",
  pink: "bg-hue-pink/12 text-hue-pink",
  orange: "bg-hue-orange/12 text-hue-orange",
};

/** An icon on a soft tile of its colour. */
export function IconTile({ icon, hue = "blue", size = "md" }: { icon: ReactNode; hue?: Hue; size?: "sm" | "md" | "lg" }) {
  return (
    <span
      className={cx(
        "inline-flex shrink-0 items-center justify-center",
        size === "sm" ? "h-6 w-6 rounded-lg [&>svg]:h-3.5 [&>svg]:w-3.5" : size === "lg" ? "h-11 w-11 rounded-2xl [&>svg]:h-5.5 [&>svg]:w-5.5" : "h-8 w-8 rounded-xl [&>svg]:h-4 [&>svg]:w-4",
        HUE[hue],
      )}
    >
      {icon}
    </span>
  );
}

/** A titled panel. */
export function Card({
  title,
  icon,
  hue,
  extra,
  children,
  className,
  bodyClassName,
  tone,
}: {
  title?: ReactNode;
  icon?: ReactNode;
  /** With an icon: its colour tile. */
  hue?: Hue;
  extra?: ReactNode;
  children: ReactNode;
  className?: string;
  bodyClassName?: string;
  tone?: Tone;
}) {
  return (
    <section
      className={cx(
        "rounded-[var(--radius-card)] border bg-surface shadow-[var(--shadow-card)]",
        tone === "bad" ? "border-bad/40" : tone === "warn" ? "border-warn/40" : "border-line",
        className,
      )}
    >
      {(title || extra) && (
        <header className="flex items-center gap-2.5 border-b border-line/70 px-5 py-3.5">
          {icon && (hue ? <IconTile icon={icon} hue={hue} size="sm" /> : <span className="text-ink-muted">{icon}</span>)}
          <h2 className="text-[15px] font-medium text-ink">{title}</h2>
          <div className="ml-auto flex items-center gap-2 text-xs text-ink-muted">{extra}</div>
        </header>
      )}
      <div className={cx("p-5", bodyClassName)}>{children}</div>
    </section>
  );
}

/** A value as a ring: how full something is, at a glance. */
export function Ring({ value, max = 100, tone = "accent", size = 72, label }: { value: number | null; max?: number; tone?: Tone | Hue; size?: number; label?: ReactNode }) {
  const pct = value === null ? 0 : Math.max(0, Math.min(1, value / max));
  const r = 30;
  const c = 2 * Math.PI * r;
  const color =
    tone === "good" || tone === "warn" || tone === "bad" || tone === "accent" ? `var(--color-${tone})` : tone === "neutral" ? "var(--color-ink-faint)" : `var(--color-hue-${tone})`;
  return (
    <div className="relative inline-flex items-center justify-center" style={{ width: size, height: size }}>
      <svg viewBox="0 0 72 72" className="h-full w-full -rotate-90">
        <circle cx="36" cy="36" r={r} fill="none" stroke="var(--color-line)" strokeWidth="7" />
        <circle
          cx="36"
          cy="36"
          r={r}
          fill="none"
          stroke={color}
          strokeWidth="7"
          strokeLinecap="round"
          strokeDasharray={`${c * pct} ${c}`}
          style={{ transition: "stroke-dasharray 0.6s ease" }}
        />
      </svg>
      <div className="absolute text-center text-sm font-semibold text-ink">{label ?? (value === null ? "—" : `${Math.round(pct * 100)}%`)}</div>
    </div>
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
  hue = "blue",
  trend,
}: {
  label: ReactNode;
  value: ReactNode;
  sub?: ReactNode;
  tone?: Tone;
  help?: string;
  icon?: ReactNode;
  hue?: Hue;
  /** A small chart under the number: how it got here. */
  trend?: ReactNode;
}) {
  return (
    <div className="flex flex-col rounded-[var(--radius-card)] border border-line bg-surface px-5 py-4 shadow-[var(--shadow-card)]">
      <div className="flex items-center gap-2.5 text-[13px] text-ink-muted">
        {icon && <IconTile icon={icon} hue={hue} />}
        <span>{label}</span>
        {help && <Help term={help} />}
      </div>
      <div className={cx("mt-3 truncate text-[26px] font-medium leading-tight tracking-tight", TONE_TEXT[tone ?? "neutral"])}>{value}</div>
      {sub && <div className="mt-1 truncate text-xs text-ink-muted">{sub}</div>}
      {trend && <div className="-mx-1 mt-2">{trend}</div>}
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
  primary: "bg-accent text-white shadow-sm hover:bg-accent/90",
  secondary: "border border-line-strong bg-surface text-accent hover:bg-accent-soft",
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
        "inline-flex items-center justify-center gap-1.5 rounded-full font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40",
        size === "sm" ? "h-7 px-3 text-xs" : "h-9 px-4 text-sm",
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
  const body = text ?? (term ? glossary(term) : undefined);
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
          <button className="ml-auto rounded p-1 text-ink-muted hover:bg-surface-hover hover:text-ink" onClick={onClose} aria-label={tr("关闭", "Close")}>
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
      title={tr("刷新", "Refresh")}
      className={cx("inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs hover:bg-surface-hover", age > staleAfterS ? "text-warn" : "text-ink-faint")}
    >
      {fetching ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
      {age < 2 ? tr("刚刚更新", "Just updated") : tr(`${fmtDuration(age)}前更新`, `Updated ${fmtDuration(age)} ago`)}
    </button>
  );
}

// -- formatting --------------------------------------------------------------

export function fmtDuration(seconds: number): string {
  const s = Math.round(Math.abs(seconds));
  const [h, m, d, hd] = [Math.floor(s / 3600), Math.floor((s % 3600) / 60), Math.floor(s / 86400), Math.floor((s % 86400) / 3600)];
  if (s < 60) return tr(`${s} 秒`, `${s}s`);
  if (s < 3600) return tr(`${Math.floor(s / 60)} 分钟`, `${Math.floor(s / 60)} min`);
  if (s < 86400) return m ? tr(`${h} 小时 ${m} 分`, `${h}h ${m}m`) : tr(`${h} 小时`, `${h}h`);
  return hd ? tr(`${d} 天 ${hd} 小时`, `${d}d ${hd}h`) : tr(`${d} 天`, `${d}d`);
}

export function fmtTime(ms: number, withDate = true): string {
  return new Date(ms).toLocaleString(intlLocale(), withDate ? { hour12: false } : { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/** How long ago, as a phrase: "3 分钟前" / "3 min ago". */
export function agoText(seconds: number): string {
  return seconds < 5 ? tr("刚刚", "just now") : tr(`${fmtDuration(seconds)}前`, `${fmtDuration(seconds)} ago`);
}

/** "3 分钟前", with the exact time on hover. */
export function Ago({ ms }: { ms: number }) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 5000);
    return () => clearInterval(t);
  }, []);
  const s = (now - ms) / 1000;
  return <span title={fmtTime(ms)}>{agoText(s)}</span>;
}

export function fmtBytes(b: number | null | undefined): string {
  if (b == null) return "—";
  if (b >= 2 ** 30) return `${(b / 2 ** 30).toFixed(1)} GiB`;
  if (b >= 2 ** 20) return `${(b / 2 ** 20).toFixed(1)} MiB`;
  if (b >= 2 ** 10) return `${(b / 2 ** 10).toFixed(1)} KiB`;
  return `${b} B`;
}

/**
 * The product mark: four tiles in Google's colours, drawn rather than
 * loaded. The sign-in screens and the sidebar draw the same one, so the
 * door and the room behind it carry the same mark.
 */
export function BrandMark({ className }: { className?: string }) {
  return (
    <span className={cx("grid grid-cols-2 gap-[3px] rounded-lg p-[3px]", className)} aria-hidden>
      <span className="rounded-[4px] bg-hue-blue" />
      <span className="rounded-[4px] bg-hue-red" />
      <span className="rounded-[4px] bg-hue-yellow" />
      <span className="rounded-[4px] bg-hue-green" />
    </span>
  );
}

/** A signed money amount, coloured by sign only when asked. */
export function Money({ value, signed }: { value: string | number | null | undefined; signed?: boolean }) {
  if (value == null || value === "") return <span className="text-ink-faint">—</span>;
  const n = Number(value);
  const text = Number.isFinite(n) ? n.toLocaleString(intlLocale(), { maximumFractionDigits: 4 }) : String(value);
  if (!signed || !Number.isFinite(n) || n === 0) return <span>{text}</span>;
  return <span className={n > 0 ? "text-good" : "text-bad"}>{n > 0 ? `+${text}` : text}</span>;
}
