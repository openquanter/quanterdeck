import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Clock } from "lucide-react";

import { ApiError, api } from "@/api/client";
import { tr } from "@/i18n";
import { Button, cx } from "@/ui/kit";

/** The sign-in screens' text field. Shared with setup so both look alike. */
export const INPUT =
  "w-full rounded-md border border-line-strong bg-ground px-3 py-2 text-sm text-ink outline-none transition-colors placeholder:text-ink-faint focus:border-accent";

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
      <div className="space-y-4 text-sm">
        <div className="flex items-start gap-3 rounded-md border border-warn/40 bg-warn/8 px-3.5 py-3">
          <Clock className="mt-0.5 h-4 w-4 shrink-0 text-warn" />
          <div>
            <p className="font-medium text-ink">{tr("暂时无法登录。", "Sign-in is temporarily unavailable.")}</p>
            <p className="mt-1 text-ink-muted">{refusal.detail}</p>
          </div>
        </div>
        <Button onClick={() => setRefusal(null)}>{tr("稍后再试", "Try again later")}</Button>
      </div>
    );
  }

  return (
    <form onSubmit={submit} className="space-y-4 text-sm">
      <label className="block">
        <span className="mb-1.5 block text-xs text-ink-muted">{tr("密码", "Password")}</span>
        <input
          type="password"
          autoComplete="current-password"
          autoFocus
          required
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className={INPUT}
        />
      </label>
      {totpRequired && (
        <label className="block">
          <span className="mb-1.5 block text-xs text-ink-muted">{tr("验证码（6 位）", "Verification code (6 digits)")}</span>
          <input
            inputMode="numeric"
            autoComplete="one-time-code"
            pattern="[0-9]{6}"
            maxLength={6}
            required
            value={totp}
            onChange={(e) => setTotp(e.target.value.replace(/\D/g, ""))}
            className={cx(INPUT, "font-mono tracking-[0.4em]")}
          />
        </label>
      )}
      {refusal && (
        <p className="rounded-md border border-bad/40 bg-bad/10 px-3 py-2 text-bad">
          {refusal.status === 401 ? tr("密码或验证码不正确。", "Incorrect password or code.") : refusal.detail}
        </p>
      )}
      {/* The kit's Button at full width: a sign-in form's one action. */}
      <div className="[&>button]:h-9 [&>button]:w-full">
        <Button type="submit" variant="primary" disabled={busy}>
          {busy ? tr("正在登录…", "Signing in…") : tr("登录", "Sign in")}
        </Button>
      </div>
    </form>
  );
}
