import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { Monitor, Moon, Sun } from "lucide-react";

import { cx } from "./kit";

/**
 * Light, dark, or whatever the system says. A reading preference, kept in
 * this browser; the page is correct without it.
 */
export type ThemeChoice = "light" | "dark" | "system";
type Resolved = "light" | "dark";

const KEY = "oq-deck-theme";

function stored(): ThemeChoice {
  try {
    const v = localStorage.getItem(KEY);
    return v === "dark" || v === "system" || v === "light" ? v : "light";
  } catch {
    return "light";
  }
}

function systemDark(): boolean {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches === true;
}

function resolve(choice: ThemeChoice): Resolved {
  return choice === "system" ? (systemDark() ? "dark" : "light") : choice;
}

/** Set before the first render, so the page does not flash the other theme. */
export function applyStoredTheme() {
  document.documentElement.dataset.theme = resolve(stored());
}

const Ctx = createContext<{ choice: ThemeChoice; resolved: Resolved; setChoice: (c: ThemeChoice) => void }>({
  choice: "light",
  resolved: "light",
  setChoice: () => undefined,
});

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [choice, setChoice] = useState<ThemeChoice>(stored);
  const [dark, setDark] = useState(systemDark);
  useEffect(() => {
    const m = window.matchMedia?.("(prefers-color-scheme: dark)");
    const on = () => setDark(m.matches);
    m?.addEventListener("change", on);
    return () => m?.removeEventListener("change", on);
  }, []);
  const resolved: Resolved = choice === "system" ? (dark ? "dark" : "light") : choice;
  useEffect(() => {
    document.documentElement.dataset.theme = resolved;
    try {
      localStorage.setItem(KEY, choice);
    } catch {
      /* private window: the choice lasts this page only */
    }
  }, [choice, resolved]);
  const value = useMemo(() => ({ choice, resolved, setChoice }), [choice, resolved]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useTheme() {
  return useContext(Ctx);
}

/** The palette as the current theme defines it, for things drawn on a canvas. */
export function useThemeColors() {
  const { resolved } = useTheme();
  return useMemo(() => {
    const css = getComputedStyle(document.documentElement);
    const v = (name: string) => css.getPropertyValue(name).trim();
    return {
      theme: resolved,
      ink: v("--color-ink"),
      muted: v("--color-ink-muted"),
      faint: v("--color-ink-faint"),
      line: v("--color-line"),
      lineStrong: v("--color-line-strong"),
      surface: v("--color-surface"),
      raised: v("--color-surface-raised"),
      accent: v("--color-accent"),
      good: v("--color-good"),
      warn: v("--color-warn"),
      bad: v("--color-bad"),
      series: ["--color-hue-blue", "--color-hue-purple", "--color-hue-teal", "--color-hue-orange", "--color-hue-pink", "--color-hue-green"].map(v),
    };
    // Re-read when the theme changes; the variables are the source.
  }, [resolved]);
}

/** Light / dark / system, as three small buttons. */
export function ThemeToggle() {
  const { choice, setChoice } = useTheme();
  const opts: { v: ThemeChoice; icon: ReactNode; label: string }[] = [
    { v: "light", icon: <Sun className="h-3.5 w-3.5" />, label: "浅色" },
    { v: "dark", icon: <Moon className="h-3.5 w-3.5" />, label: "深色" },
    { v: "system", icon: <Monitor className="h-3.5 w-3.5" />, label: "跟随系统" },
  ];
  return (
    <div className="inline-flex rounded-full border border-line bg-surface p-0.5">
      {opts.map((o) => (
        <button
          key={o.v}
          title={o.label}
          onClick={() => setChoice(o.v)}
          className={cx("rounded-full p-1.5 transition-colors", choice === o.v ? "bg-accent-soft text-accent" : "text-ink-faint hover:text-ink")}
        >
          {o.icon}
        </button>
      ))}
    </div>
  );
}
