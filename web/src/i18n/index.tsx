import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

/**
 * Chinese and English, side by side where the words are used.
 *
 * Every piece of interface copy is written as `tr("中文", "English")`:
 * both languages at the call site, so a string cannot exist in one and be
 * forgotten in the other, and a reviewer reads the pair together. There
 * are no keys to look up and no file of translations to drift from the
 * screens they belong to.
 *
 * `tr` reads the current language from this module rather than from a
 * hook, so constants built at render time (tab labels, glossary lines)
 * use it too. Switching language remounts the application under a new
 * key (see `I18nProvider`), which is what makes every `tr` run again.
 *
 * What is **not** translated is data: log lines, journal fields, a
 * venue's own error text, identifiers. Those are shown as they are.
 */

export type Locale = "zh" | "en";

const KEY = "oq-deck-lang";

function initial(): Locale {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "zh" || v === "en") return v;
  } catch {
    /* private window */
  }
  return typeof navigator !== "undefined" && !navigator.language.toLowerCase().startsWith("zh") ? "en" : "zh";
}

let current: Locale = initial();

/** The interface language now. */
export function locale(): Locale {
  return current;
}

/** The copy in the current language. */
export function tr(zh: string, en: string): string {
  return current === "en" ? en : zh;
}

/** The BCP 47 tag for dates and numbers in the current language. */
export function intlLocale(): string {
  return current === "en" ? "en-US" : "zh-CN";
}

/** What the deck is asked to answer in: its refusals and explanations. */
export function acceptLanguage(): string {
  return current === "en" ? "en" : "zh-CN";
}

const Ctx = createContext<{ locale: Locale; setLocale: (l: Locale) => void }>({ locale: current, setLocale: () => undefined });

/**
 * Holds the language and remounts its children when it changes, so that
 * every `tr` call runs again in the new one.
 */
export function I18nProvider({ children }: { children: (locale: Locale) => ReactNode }) {
  const [loc, setLoc] = useState<Locale>(current);
  current = loc;
  useEffect(() => {
    document.documentElement.lang = loc === "en" ? "en" : "zh-CN";
    try {
      localStorage.setItem(KEY, loc);
    } catch {
      /* the choice lasts this page only */
    }
  }, [loc]);
  const value = useMemo(() => ({ locale: loc, setLocale: (l: Locale) => setLoc(l) }), [loc]);
  return <Ctx.Provider value={value}>{children(loc)}</Ctx.Provider>;
}

export function useLocale() {
  return useContext(Ctx);
}

/** Each language named in itself, whatever the interface is in. */
const NAMES: Record<Locale, string> = { zh: "中文", en: "English" }; // i18n-ok
const SHORT: Record<Locale, string> = { zh: "中", en: "EN" }; // i18n-ok

/** 中 / EN, as two small buttons. */
export function LanguageToggle() {
  const { locale: loc, setLocale } = useLocale();
  return (
    <div className="inline-flex rounded-full border border-line bg-surface p-0.5 text-[11px] font-medium">
      {(["zh", "en"] as Locale[]).map((l) => (
        <button
          key={l}
          title={NAMES[l]}
          onClick={() => setLocale(l)}
          className={`rounded-full px-2 py-1 transition-colors ${loc === l ? "bg-accent-soft text-accent" : "text-ink-faint hover:text-ink"}`}
        >
          {SHORT[l]}
        </button>
      ))}
    </div>
  );
}
