import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ShieldAlert, TriangleAlert } from "lucide-react";

import { api, ApiError, type OpsAction } from "@/api/client";
import { Button, cx } from "@/ui/kit";

/**
 * Every state change goes through this: what it will do, a reason, an
 * explicit confirmation, and for the risky ones the step-up code the host
 * agent checks. The reason is not decoration — it is what the audit trail,
 * the trader's journal and the alert channel will say about this moment.
 */
export function ActionDialog({
  title,
  consequence,
  action,
  highRisk,
  onClose,
}: {
  title: string;
  consequence: string;
  action: OpsAction;
  highRisk: boolean;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [reason, setReason] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ready = reason.trim().length > 0 && confirmed && (!highRisk || /^\d{6}$/.test(code)) && !busy;

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      await api.act(action, reason.trim(), highRisk ? code : "");
      await queryClient.invalidateQueries();
      onClose();
    } catch (e) {
      setError(e instanceof ApiError ? e.detail : String(e));
    } finally {
      setBusy(false);
    }
  }

  const input = "mt-1.5 w-full rounded-md border border-line-strong bg-ground px-3 py-2 text-sm text-ink outline-none focus:border-accent";
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm" onClick={() => !busy && onClose()}>
      <div className="w-full max-w-md rounded-xl border border-line-strong bg-surface shadow-2xl" onClick={(e) => e.stopPropagation()}>
        <div className="flex gap-3 px-5 pt-5">
          <div className={cx("flex h-9 w-9 shrink-0 items-center justify-center rounded-full", highRisk ? "bg-bad/15 text-bad" : "bg-warn/15 text-warn")}>
            {highRisk ? <ShieldAlert className="h-5 w-5" /> : <TriangleAlert className="h-5 w-5" />}
          </div>
          <div>
            <h2 className="text-base font-semibold text-ink">{title}</h2>
            <p className="mt-1 text-sm leading-relaxed text-ink-muted">{consequence}</p>
          </div>
        </div>

        <div className="space-y-4 px-5 py-4">
          <label className="block text-xs text-ink-muted">
            原因 <span className="text-ink-faint">· 必填，写进审计、交易日志和告警频道</span>
            <textarea className={input} rows={2} value={reason} onChange={(e) => setReason(e.target.value)} autoFocus />
          </label>

          {highRisk && (
            <label className="block text-xs text-ink-muted">
              二次验证码 <span className="text-ink-faint">· 验证器里「oq-agent」那一项的 6 位数字</span>
              <input
                className={cx(input, "w-44 font-mono tracking-[0.4em]")}
                inputMode="numeric"
                maxLength={6}
                value={code}
                onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
              />
            </label>
          )}

          <label className="flex items-center gap-2 text-sm text-ink">
            <input type="checkbox" className="h-4 w-4 accent-[var(--color-accent)]" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />
            我确认执行「{title}」
          </label>

          {error && <p className="rounded-md border border-bad/40 bg-bad/10 px-3 py-2 text-sm text-bad">{error}</p>}
        </div>

        <div className="flex justify-end gap-2 border-t border-line px-5 py-3">
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            取消
          </Button>
          <Button variant={highRisk ? "danger" : "primary"} disabled={!ready} onClick={submit}>
            {busy ? "执行中…" : "执行"}
          </Button>
        </div>
      </div>
    </div>
  );
}
