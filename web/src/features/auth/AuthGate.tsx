import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Navigate, useLocation } from "react-router-dom";

import { UNAUTHENTICATED, api } from "@/api/client";

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
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-ground/80 p-6">
          <div className="w-full max-w-sm rounded border border-line bg-surface p-6">
            <h2 className="mb-1 font-mono text-sm text-ink">会话已结束</h2>
            <p className="mb-4 text-xs text-ink-muted">重新登录后回到这一页，内容不会丢。</p>
            <LoginForm
              totpRequired={session.totp_required}
              onDone={() => {
                setExpired(false);
                void queryClient.invalidateQueries();
              }}
            />
          </div>
        </div>
      )}
    </>
  );
}
