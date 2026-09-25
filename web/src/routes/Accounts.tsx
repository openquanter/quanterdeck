import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, KeyRound, ShieldAlert, XCircle } from "lucide-react";

import { api } from "@/api/client";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { tr } from "@/i18n";
import { Badge, Card, Freshness, PageHeader, Table, cx } from "@/ui/kit";

/**
 * Which venue account each process is using (blueprint §7 exchanges):
 * the key's fingerprint, never the key. The console holds no key and
 * cannot ask the venue about permissions; it can show whether the trader
 * and its watcher are looking at the same account, which is the mistake
 * that has actually happened — so that verdict leads the page.
 */
export function Accounts() {
  const q = useQuery({ queryKey: ["ops", "accounts"], queryFn: api.accounts, refetchInterval: 60_000 });
  const entries = Object.entries(q.data?.processes ?? {});
  const same = q.data?.same_account === true;

  return (
    <div className="space-y-5">
      <PageHeader
        title={tr("交易所账户", "Exchange accounts")}
        description={tr("每个进程所用密钥的指纹，以及它们是否指向同一个账户。", "The fingerprint of the key each process uses, and whether they all point at the same account.")}
        meta={<Freshness at={q.dataUpdatedAt} fetching={q.isFetching} staleAfterS={120} onRefresh={() => void q.refetch()} />}
      />

      {q.isLoading ? (
        <Skeleton tiles={1} rows={4} />
      ) : q.isError ? (
        <ErrorState error={q.error} what={tr("账户", "accounts")} />
      ) : entries.length === 0 ? (
        <Empty
          title={tr("日志里还没有账户指纹。", "No account fingerprints in the logs yet.")}
          next={tr(
            "交易进程和 oq-recon 启动时把所用密钥的指纹写在日志第一行。",
            "The trading process and oq-recon write their key's fingerprint on the first log line at startup.",
          )}
        />
      ) : (
        <>
          <div
            className={cx(
              "flex items-start gap-3 rounded-[var(--radius-card)] border px-5 py-4",
              same ? "border-good/30 bg-good/6" : "border-bad/40 bg-bad/8",
            )}
          >
            {same ? <CheckCircle2 className="mt-0.5 h-6 w-6 shrink-0 text-good" /> : <XCircle className="mt-0.5 h-6 w-6 shrink-0 text-bad" />}
            <div>
              <div className="text-lg font-semibold text-ink">{same ? tr("同一个账户", "Same account") : tr("不是同一个账户", "Different accounts")}</div>
              <p className="mt-0.5 text-sm text-ink-muted">
                {same
                  ? tr("所有进程用的是同一个账户。", "All processes use the same account.")
                  : tr(
                      "进程用的不是同一个账户：对账进程看的可能不是交易进程在交易的账户。",
                      "The processes use different accounts: reconciliation may be watching an account other than the one being traded.",
                    )}
              </p>
            </div>
          </div>

          <Card title={tr("各进程的密钥指纹", "Key fingerprint per process")} icon={<KeyRound className="h-4 w-4" />} bodyClassName="p-0">
            <Table head={[tr("进程", "Process"), tr("密钥指纹", "Key fingerprint"), tr("来源", "Source")]}>
              {entries.map(([p, v]) => (
                <tr key={p}>
                  <td className="text-ink">{p}</td>
                  <td>{v.fingerprint ? <span className="font-mono text-ink">{v.fingerprint}</span> : <Badge tone="warn">{tr("未写", "Not written")}</Badge>}</td>
                  <td className="font-mono text-xs text-ink-faint">{v.log}</td>
                </tr>
              ))}
            </Table>
          </Card>
        </>
      )}

      <Card title={tr("密钥与提币权限", "Keys and withdrawal permission")} icon={<ShieldAlert className="h-4 w-4" />}>
        <p className="text-sm leading-relaxed text-ink-muted">
          {tr(
            "密钥本身只在交易进程的 systemd 凭据里，控制台和主机代理都读不到。API 权限（是否允许提币）只能在交易所后台或主网 API 查询，测试网不提供这个接口；",
            "The key itself lives only in the trading process's systemd credentials; neither the console nor the host agent can read it. API permissions (whether withdrawals are allowed) can only be checked in the exchange's dashboard or through the mainnet API; testnet has no such endpoint. ",
          )}
          <span className="text-ink">
            {tr("上主网前请在交易所后台确认密钥没有提币权限。", "Before going to mainnet, confirm in the exchange's dashboard that the key cannot withdraw.")}
          </span>
        </p>
      </Card>
    </div>
  );
}
