import { useQuery } from "@tanstack/react-query";
import { Navigate, useNavigate, useSearchParams } from "react-router-dom";

import { api } from "@/api/client";

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
    <Centered title="登录 quanterdeck">
      {session ? (
        <LoginForm totpRequired={session.totp_required} onDone={() => navigate(next, { replace: true })} />
      ) : (
        <div className="h-24 animate-pulse rounded bg-surface-raised" />
      )}
    </Centered>
  );
}

/** Only a path inside the deck; a `next` pointing elsewhere is dropped. */
function safeNext(next: string | null): string {
  return next && next.startsWith("/") && !next.startsWith("//") ? next : "/";
}

export function Centered({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex min-h-screen items-center justify-center bg-ground p-6">
      <div className="w-full max-w-sm rounded border border-line bg-surface p-6">
        <h1 className="mb-4 font-mono text-sm text-ink">{title}</h1>
        {children}
      </div>
    </div>
  );
}
