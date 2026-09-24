import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { ApiError, api } from "@/api/client";

/**
 * Password, and a six-digit code when the deck has a second factor.
 *
 * Used by the login page and by the dialog that appears when a session
 * ends under a page, so both behave the same. A wrong password and a
 * wrong code read identically — telling them apart is how an attacker
 * learns which half they have.
 */
export function LoginForm({ totpRequired, onDone }: { totpRequired: boolean; onDone: () => void }) {
  const queryClient = useQueryClient();
  const [password, setPassword] = useState("");
  const [totp, setTotp] = useState("");
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<ApiError | null>(null);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    setRefusal(null);
    try {
      await api.login(password, totp);
      await queryClient.invalidateQueries();
      onDone();
    } catch (error) {
      setRefusal(error instanceof ApiError ? error : new ApiError(0, String(error)));
    } finally {
      setBusy(false);
      setPassword("");
      setTotp("");
    }
  }

  if (refusal?.status === 429) {
    // Its own screen: a lockout is not a wrong password, and the form
    // would invite the attempt that extends nothing but frustration.
    return (
      <div className="space-y-3 text-sm">
        <p className="text-ink">暂时无法登录。</p>
        <p className="text-ink-muted">{refusal.detail}</p>
        <button
          type="button"
          className="rounded border border-line px-3 py-1.5 text-ink-muted hover:text-ink"
          onClick={() => setRefusal(null)}
        >
          稍后再试
        </button>
      </div>
    );
  }

  return (
    <form onSubmit={submit} className="space-y-3 text-sm">
      <label className="block">
        <span className="mb-1 block text-ink-muted">密码</span>
        <input
          type="password"
          autoComplete="current-password"
          autoFocus
          required
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className="w-full rounded border border-line bg-ground px-3 py-2 text-ink"
        />
      </label>
      {totpRequired && (
        <label className="block">
          <span className="mb-1 block text-ink-muted">验证码（6 位）</span>
          <input
            inputMode="numeric"
            autoComplete="one-time-code"
            pattern="[0-9]{6}"
            maxLength={6}
            required
            value={totp}
            onChange={(e) => setTotp(e.target.value.replace(/\D/g, ""))}
            className="w-full rounded border border-line bg-ground px-3 py-2 font-mono tracking-widest text-ink"
          />
        </label>
      )}
      {refusal && (
        <p className="text-bad">
          {refusal.status === 401 ? "密码或验证码不正确。" : refusal.detail}
        </p>
      )}
      <button
        type="submit"
        disabled={busy}
        className="w-full rounded bg-accent px-3 py-2 text-ground disabled:opacity-50"
      >
        {busy ? "正在登录…" : "登录"}
      </button>
    </form>
  );
}
