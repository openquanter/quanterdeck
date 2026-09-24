import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Navigate } from "react-router-dom";

import { ApiError, api, type SetupDone } from "@/api/client";

import { Centered } from "./Login";

/**
 * `/setup`: the first run, in three steps (UI-BRIEF §5.1).
 *
 * The token is printed to the terminal the deck was started from, so
 * reading it takes the local access the operator already has. The deck
 * returns the credentials rather than keeping them: this build has no
 * configuration file, and the operator puts them in the environment.
 */
export function Setup() {
  const { data: session } = useQuery({ queryKey: ["session"], queryFn: api.session });
  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [token, setToken] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [done, setDone] = useState<SetupDone | null>(null);

  if (session && !session.setup_required && !done) return <Navigate to="/login" replace />;

  async function finish(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    setRefusal(null);
    try {
      setDone(await api.setup(token.trim(), password));
      setStep(3);
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        // The token, not the password: back to the step that has it.
        setStep(1);
      }
      setRefusal(error instanceof ApiError ? error.detail : String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Centered title={`首次设置 · 第 ${step} 步，共 3 步`}>
      {step === 1 && (
        <form
          className="space-y-3 text-sm"
          onSubmit={(e) => {
            e.preventDefault();
            setStep(2);
          }}
        >
          <p className="text-ink-muted">
            粘贴启动 deck 的那个终端里打印的一次性令牌。它只出现在那里，是因为读到它需要这台机器的本地访问权限；
            deck 重启后它就失效，设置完成后也立即失效。
          </p>
          <input
            autoFocus
            required
            value={token}
            onChange={(e) => setToken(e.target.value)}
            className="w-full rounded border border-line bg-ground px-3 py-2 font-mono text-ink"
          />
          {refusal && <p className="text-bad">{refusal}</p>}
          <button type="submit" className="w-full rounded bg-accent px-3 py-2 text-ground">
            下一步
          </button>
        </form>
      )}

      {step === 2 && (
        <form className="space-y-3 text-sm" onSubmit={finish}>
          <p className="text-ink-muted">
            设置密码。唯一的规则是至少 12 个字符——用一句只有你记得的话，比一串写在便签上的符号更好。
          </p>
          <input
            type="password"
            autoComplete="new-password"
            autoFocus
            required
            minLength={12}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="w-full rounded border border-line bg-ground px-3 py-2 text-ink"
          />
          <p className="text-xs text-ink-muted">{password.length} / 至少 12</p>
          {refusal && <p className="text-bad">{refusal}</p>}
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => setStep(1)}
              className="rounded border border-line px-3 py-2 text-ink-muted hover:text-ink"
            >
              上一步
            </button>
            <button
              type="submit"
              disabled={busy || password.length < 12}
              className="flex-1 rounded bg-accent px-3 py-2 text-ground disabled:opacity-50"
            >
              {busy ? "正在生成…" : "生成凭据"}
            </button>
          </div>
        </form>
      )}

      {step === 3 && done && <Credentials done={done} />}
    </Centered>
  );
}

function Credentials({ done }: { done: SetupDone }) {
  const uri = `otpauth://totp/quanterdeck?secret=${done.totp_secret}&issuer=quanterdeck`;
  return (
    <div className="space-y-4 text-sm">
      <p className="rounded border border-warn/40 bg-warn/10 p-3 text-ink">
        下面两样东西<strong>不要截图、不要粘进聊天、不要提交进 git</strong>。离开这一页就不会再显示。
      </p>
      <Field label="OQ_DECK_PASSWORD_HASH" value={done.password_hash} />
      <Field label="OQ_DECK_TOTP_SECRET（录入验证器应用）" value={done.totp_secret} />
      <Field label="验证器链接（支持直接打开的应用可用）" value={uri} />
      <ol className="list-decimal space-y-1 pl-5 text-ink-muted">
        {done.next_steps.map((step) => (
          <li key={step}>{step}</li>
        ))}
      </ol>
    </div>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="mb-1 text-xs text-ink-muted">{label}</div>
      <div className="break-all rounded border border-line bg-ground p-2 font-mono text-xs text-ink">
        {value}
      </div>
    </div>
  );
}
