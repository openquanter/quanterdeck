import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { api, ApiError, type OpsAction } from "@/api/client";

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

  const ready =
    reason.trim().length > 0 && confirmed && (!highRisk || /^\d{6}$/.test(code)) && !busy;

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

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div className="w-full max-w-md rounded border border-line bg-surface p-5">
        <h2 className="text-base text-ink">{title}</h2>
        <p className="mt-2 text-sm text-ink-muted">{consequence}</p>

        <label className="mt-4 block text-xs text-ink-muted">
          原因（必填，写进审计与交易日志）
          <textarea
            className="mt-1 w-full rounded border border-line bg-ground p-2 text-sm text-ink"
            rows={2}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
        </label>

        {highRisk && (
          <label className="mt-3 block text-xs text-ink-muted">
            二次验证码（验证器里「oq-agent」那一项的 6 位数字）
            <input
              className="mt-1 w-40 rounded border border-line bg-ground p-2 font-mono text-sm tracking-widest text-ink"
              inputMode="numeric"
              maxLength={6}
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, ""))}
            />
          </label>
        )}

        <label className="mt-3 flex items-center gap-2 text-sm text-ink">
          <input
            type="checkbox"
            checked={confirmed}
            onChange={(e) => setConfirmed(e.target.checked)}
          />
          我确认执行：{title}
        </label>

        {error && <p className="mt-3 text-sm text-bad">{error}</p>}

        <div className="mt-5 flex justify-end gap-2">
          <button
            className="rounded border border-line px-3 py-1.5 text-sm text-ink-muted hover:text-ink"
            onClick={onClose}
            disabled={busy}
          >
            取消
          </button>
          <button
            className={[
              "rounded px-3 py-1.5 text-sm",
              ready ? "bg-bad text-white" : "bg-surface-raised text-ink-muted",
            ].join(" ")}
            disabled={!ready}
            onClick={submit}
          >
            {busy ? "执行中…" : "执行"}
          </button>
        </div>
      </div>
    </div>
  );
}
