import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router-dom";
import { KeyRound } from "lucide-react";

import { ApiError, api } from "@/api/client";
import { tr } from "@/i18n";
import { Button } from "@/ui/kit";

import { Centered } from "./Login";

/**
 * `/enrol?code=…` — the browser's half of enrolling by SSH key.
 *
 * `scripts/deck-enrol.sh` proves the key on the machine and prints this
 * link; opening it here spends the code and leaves a device cookie. The
 * two halves are separate because they happen in different processes,
 * and because neither is worth anything alone: the code is good once,
 * for two minutes, and for one browser.
 */
export function Enrol() {
  const [params] = useSearchParams();
  const code = params.get("code") ?? "";
  const queryClient = useQueryClient();
  const [failure, setFailure] = useState<ApiError | null>(null);
  const [done, setDone] = useState<string | null>(null);
  // React 18 runs effects twice in development; a code is good once, so
  // the second run would report a spent code as a failure.
  const spent = useRef(false);

  useEffect(() => {
    if (!code || spent.current) return;
    spent.current = true;
    api
      .claim(code)
      .then(async (r) => {
        await queryClient.invalidateQueries();
        setDone(r.label);
      })
      .catch((error) => setFailure(error instanceof ApiError ? error : new ApiError(0, String(error))));
  }, [code, queryClient]);

  return (
    <Centered title={tr("用 SSH 密钥登记", "Enrol with an SSH key")}>
      {!code ? (
        <p className="text-sm text-ink-muted">
          {tr(
            "这个链接里没有登记码。在要登记的那台机器上运行 scripts/deck-enrol.sh，它会打印一个带码的链接。",
            "This link has no code in it. Run scripts/deck-enrol.sh on the machine you want to enrol; it prints a link that does.",
          )}
        </p>
      ) : failure ? (
        <div className="space-y-4 text-sm">
          <div className="flex items-start gap-3 rounded-md border border-warn/40 bg-warn/8 px-3.5 py-3">
            <KeyRound className="mt-0.5 h-4 w-4 shrink-0 text-warn" />
            <div>
              <p className="font-medium text-ink">
                {tr("这个登记码不能用了。", "That code cannot be used.")}
              </p>
              <p className="mt-1 text-ink-muted">{failure.detail}</p>
            </div>
          </div>
          <p className="text-xs leading-relaxed text-ink-faint">
            {tr(
              "登记码只能用一次、只活两分钟。重新运行脚本会得到一个新的。",
              "A code works once and lives two minutes. Running the script again mints a new one.",
            )}
          </p>
          <Link to="/login">
            <Button>{tr("改为用密码登录", "Sign in with a password instead")}</Button>
          </Link>
        </div>
      ) : done ? (
        <div className="space-y-4 text-sm">
          <p className="text-ink">
            {tr("这台浏览器已经登记为", "This browser is enrolled as")}{" "}
            <span className="font-medium">{done}</span>
            {tr("。", ".")}
          </p>
          <p className="text-xs leading-relaxed text-ink-faint">
            {tr(
              "之后不再要密码和验证码。它带的是设备凭证，少一个因素，所以在设置页里列得出来、随时可以撤销。",
              "It will not be asked for a password or a code again. What it holds is a device credential — one factor instead of two — which is why it is listed in Settings and can be revoked at any time.",
            )}
          </p>
          <Link to="/">
            <Button variant="primary">{tr("进入控制台", "Open the console")}</Button>
          </Link>
        </div>
      ) : (
        <p className="text-sm text-ink-muted">{tr("正在登记…", "Enrolling…")}</p>
      )}
    </Centered>
  );
}
