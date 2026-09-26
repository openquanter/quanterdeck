import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Navigate } from "react-router-dom";

import { ApiError, api, type SetupDone } from "@/api/client";
import { Copy } from "@/components/States";
import { tr } from "@/i18n";
import { Button, cx } from "@/ui/kit";

import { Centered } from "./Login";
import { INPUT } from "./LoginForm";

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

  const stepName = [tr("粘贴令牌", "Paste token"), tr("设置密码", "Set password"), tr("保存凭据", "Save credentials")][step - 1];

  return (
    <Centered
      title={tr("首次设置", "First-time setup")}
      subtitle={tr(`第 ${step} 步，共 3 步 · ${stepName}`, `Step ${step} of 3 · ${stepName}`)}
    >
      <Steps step={step} />
      {step === 1 && (
        <form
          className="space-y-4 text-sm"
          onSubmit={(e) => {
            e.preventDefault();
            setStep(2);
          }}
        >
          <p className="leading-relaxed text-ink-muted">
            {tr(
              "粘贴启动 deck 的那个终端里打印的一次性令牌。它只出现在那里，是因为读到它需要这台机器的本地访问权限；deck 重启后它就失效，设置完成后也立即失效。",
              "Paste the one-time token printed in the terminal that started the deck. It appears only there because reading it requires local access to this machine. It expires when the deck restarts, and as soon as setup completes.",
            )}
          </p>
          <input autoFocus required value={token} onChange={(e) => setToken(e.target.value)} className={cx(INPUT, "font-mono")} placeholder={tr("一次性令牌", "One-time token")} />
          {refusal && <Refusal text={refusal} />}
          <Wide>
            <Button type="submit" variant="primary">
              {tr("下一步", "Next")}
            </Button>
          </Wide>
        </form>
      )}

      {step === 2 && (
        <form className="space-y-4 text-sm" onSubmit={finish}>
          <p className="leading-relaxed text-ink-muted">
            {tr(
              "设置密码。唯一的规则是至少 12 个字符——用一句只有你记得的话，比一串写在便签上的符号更好。",
              "Set a password. The only rule is at least 12 characters. A sentence only you remember beats a string of symbols on a sticky note.",
            )}
          </p>
          <div>
            <input
              type="password"
              autoComplete="new-password"
              autoFocus
              required
              minLength={12}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className={INPUT}
            />
            <p className={cx("mt-1.5 text-xs tabular-nums", password.length >= 12 ? "text-good" : "text-ink-faint")}>{tr(`${password.length} / 至少 12`, `${password.length} / at least 12`)}</p>
          </div>
          {refusal && <Refusal text={refusal} />}
          <div className="flex gap-2">
            <Button onClick={() => setStep(1)}>{tr("上一步", "Back")}</Button>
            <div className="flex-1 [&>button]:h-8 [&>button]:w-full">
              <Button type="submit" variant="primary" disabled={busy || password.length < 12}>
                {busy ? tr("正在生成…", "Generating…") : tr("生成凭据", "Generate credentials")}
              </Button>
            </div>
          </div>
        </form>
      )}

      {step === 3 && done && <Credentials done={done} />}
    </Centered>
  );
}

/** Three segments, filled up to the current step. */
function Steps({ step }: { step: 1 | 2 | 3 }) {
  return (
    <div className="mb-5 flex gap-1.5" aria-hidden>
      {[1, 2, 3].map((n) => (
        <span key={n} className={cx("h-1 flex-1 rounded-full", n <= step ? "bg-accent" : "bg-surface-raised")} />
      ))}
    </div>
  );
}

function Refusal({ text }: { text: string }) {
  return <p className="rounded-md border border-bad/40 bg-bad/10 px-3 py-2 text-bad">{text}</p>;
}

/** The kit's Button stretched to the card's width: the step's one action. */
function Wide({ children }: { children: React.ReactNode }) {
  return <div className="[&>button]:h-9 [&>button]:w-full">{children}</div>;
}

function Credentials({ done }: { done: SetupDone }) {
  const uri = `otpauth://totp/quanterdeck?secret=${done.totp_secret}&issuer=quanterdeck`;
  return (
    <div className="space-y-4 text-sm">
      <p className="rounded-md border border-warn/40 bg-warn/8 px-3.5 py-3 leading-relaxed text-ink">
        {tr("下面两样东西", "Do ")}
        <strong>{tr("不要截图、不要粘进聊天、不要提交进 git", "not screenshot, paste into chat, or commit to git")}</strong>
        {tr("。离开这一页就不会再显示。", " the two values below. They are not shown again once you leave this page.")}
      </p>
      <Field label="OQ_DECK_PASSWORD_HASH" value={done.password_hash} />
      <Field label={tr("OQ_DECK_TOTP_SECRET（录入验证器应用）", "OQ_DECK_TOTP_SECRET (enter into your authenticator app)")} value={done.totp_secret} />
      <Field label={tr("验证器链接（支持直接打开的应用可用）", "Authenticator link (for apps that open it directly)")} value={uri} />
      <p className="text-xs leading-relaxed text-ink-muted">
        {tr(
          "上面这一项是给登录用的。主机代理另有一个、给停机与部署这类高风险操作用的，与它不同——在主机上运行 sudo ~/agent-totp-qr.sh 取得，录成单独一项。两个都录，否则到第一次部署时会卡在验证码上。",
          "The entry above is what signs you in. The host agent has its own, for high-risk actions like stopping the trader or deploying — run sudo ~/agent-totp-qr.sh on the host and enrol it as a separate entry. Enrol both, or the first deploy stops at the code.",
        )}
      </p>
      <div>
        <div className="mb-1.5 text-xs text-ink-muted">{tr("接下来", "Next steps")}</div>
        <ol className="list-decimal space-y-1 pl-5 text-ink-muted">
          {done.next_steps.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      </div>
    </div>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="mb-1.5 flex items-center text-xs text-ink-muted">
        <span className="min-w-0 flex-1 truncate">{label}</span>
        <Copy text={value} />
      </div>
      <div className="break-all rounded-md border border-line-strong bg-ground px-3 py-2 font-mono text-xs text-ink">{value}</div>
    </div>
  );
}
