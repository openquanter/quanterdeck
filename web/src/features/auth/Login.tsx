import { useQuery } from "@tanstack/react-query";
import { Navigate, useNavigate, useSearchParams } from "react-router-dom";
import { api } from "@/api/client";
import { LanguageToggle, tr } from "@/i18n";
import { BrandMark } from "@/ui/kit";
import { ThemeToggle } from "@/ui/theme";

import { LoginForm } from "./LoginForm";

/** `/login`: the only door. No "remember me", no third-party sign-in. */
export function Login() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const next = safeNext(params.get("next"));
  const { data: session } = useQuery({ queryKey: ["session"], queryFn: api.session });

  if (session?.setup_required) return <Navigate to="/setup" replace />;
  if (session?.authenticated) return <Navigate to={next} replace />;

  return (
    <Centered title={tr("登录", "Sign in")} subtitle={tr("交易主机控制台", "Trading host console")}>
      {session ? (
        <LoginForm totpRequired={session.totp_required} onDone={() => navigate(next, { replace: true })} />
      ) : (
        <div className="h-28 animate-pulse rounded-md bg-surface-raised" />
      )}
    </Centered>
  );
}

/** Only a path inside the deck; a `next` pointing elsewhere is dropped. */
function safeNext(next: string | null): string {
  return next && next.startsWith("/") && !next.startsWith("//") ? next : "/";
}

/**
 * The sign-in screens' frame: the product mark as the sidebar draws it,
 * then one card, and the language and theme switches — the only two things
 * on the page besides the form, because they are the only two that mean
 * anything before signing in.
 */
export function Centered({ title, subtitle, children }: { title: string; subtitle?: string; children: React.ReactNode }) {
  return (
    <div className="relative flex min-h-screen flex-col items-center justify-center bg-ground p-6">
      {/* The console's own two switches, reachable before signing in: which
          language and which theme you read in is not a decision to make
          after you are already through the door. */}
      <div className="absolute right-6 top-6 flex items-center gap-2">
        <LanguageToggle />
        <ThemeToggle />
      </div>
      <div className="mb-6 flex items-center gap-3">
        <BrandMark className="h-10 w-10" />
        <span className="text-xl font-medium tracking-tight text-ink">
          quanter<span className="text-accent">deck</span>
        </span>
      </div>
      <div className="w-full max-w-sm rounded-[var(--radius-card)] border border-line bg-surface shadow-[var(--shadow-card)]">
        <div className="border-b border-line px-6 py-4">
          <h1 className="text-base font-semibold text-ink">{title}</h1>
          {subtitle && <p className="mt-0.5 text-xs text-ink-muted">{subtitle}</p>}
        </div>
        <div className="px-6 py-5">{children}</div>
      </div>
    </div>
  );
}
