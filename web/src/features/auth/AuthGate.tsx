import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Navigate, useLocation } from "react-router-dom";
import { LockKeyhole } from "lucide-react";

import { UNAUTHENTICATED, api } from "@/api/client";
import { tr } from "@/i18n";

import { LoginForm } from "./LoginForm";

/**
 * Everything behind the login. First run goes to setup, a visitor goes
 * to the login page, and a session that ends while a page is open is
 * answered with a dialog over that page — the operator keeps what they
 * were looking at (UI-BRIEF §5.3).
 */
export function AuthGate({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  const queryClient = useQueryClient();
  const { data: session, isLoading } = useQuery({ queryKey: ["session"], queryFn: api.session });
  const [expired, setExpired] = useState(false);

  useEffect(() => {
    const onExpired = () => setExpired(true);
    window.addEventListener(UNAUTHENTICATED, onExpired);
    return () => window.removeEventListener(UNAUTHENTICATED, onExpired);
  }, []);

  if (isLoading || !session) {
    return <div className="min-h-screen bg-ground" />;
  }
  if (session.setup_required) return <Navigate to="/setup" replace />;
  if (!session.authenticated) {
    const next = encodeURIComponent(location.pathname + location.search);
    return <Navigate to={`/login?next=${next}`} replace />;
  }

  return (
    <>
      {children}
      {expired && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6 backdrop-blur-sm">
          <div className="w-full max-w-sm rounded-[var(--radius-card)] border border-line-strong bg-surface shadow-[var(--shadow-card)]">
            <div className="flex gap-3 border-b border-line px-5 py-4">
              <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-warn/15 text-warn">
                <LockKeyhole className="h-4.5 w-4.5" />
              </span>
              <div>
                <h2 className="text-base font-semibold text-ink">{tr("会话已结束", "Session ended")}</h2>
                <p className="mt-0.5 text-xs text-ink-muted">
                  {tr("重新登录后回到这一页，内容不会丢。", "Sign in again to return to this page. Nothing is lost.")}
                </p>
              </div>
            </div>
            <div className="px-5 py-4">
              <LoginForm
                totpRequired={session.totp_required}
                onDone={() => {
                  setExpired(false);
                  void queryClient.invalidateQueries();
                }}
              />
            </div>
          </div>
        </div>
      )}
    </>
  );
}
