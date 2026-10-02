import type { ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { Database, Info, LockKeyhole, LogOut, Monitor, PenLine } from "lucide-react";

import { api } from "@/api/client";
import { ErrorState, Skeleton } from "@/components/States";
import { tr, useLocale, type Locale } from "@/i18n";
import { Badge, Button, Card, KV, PageHeader, Segmented, agoText } from "@/ui/kit";
import { useTheme, type ThemeChoice } from "@/ui/theme";

/**
 * What this deck is, how it is reached, and what it may do (UI-BRIEF
 * §4.7). Configuration lives in the service's environment, so this page
 * shows it and says which variable changes it — a console that edited
 * its own security settings from inside a session would be one a stolen
 * session could loosen.
 */
/**
 * The browsers the operator has enrolled, and how to take one away.
 *
 * Revocation lives next to the setting that makes it possible, because a
 * device that cannot be taken away is a second factor the operator
 * cannot get back — and this is the page they would come to.
 */
function EnrolledBrowsers({ can, days, trustedKeys }: { can: boolean; days: number; trustedKeys: string | null }) {
  const queryClient = useQueryClient();
  const devices = useQuery({ queryKey: ["devices"], queryFn: api.devices, enabled: can });
  const revoke = async (id: string) => {
    await api.revokeDevice(id).catch(() => undefined);
    await queryClient.invalidateQueries({ queryKey: ["devices"] });
  };

  return (
    <Card title={tr("已登记的浏览器", "Enrolled browsers")} icon={<Monitor className="h-4 w-4" />}>
      {!can ? (
        <p className="text-sm leading-relaxed text-ink-muted">
          {tr(
            "这台 deck 没有可写的状态目录（OQ_DECK_STATE_DIR，或 systemd 的 StateDirectory=），所以记不住任何浏览器——登录页也不会给出「记住这台设备」。",
            "This deck has no writable state directory (OQ_DECK_STATE_DIR, or systemd's StateDirectory=), so it cannot remember a browser — the login page does not offer it either.",
          )}
        </p>
      ) : (devices.data?.devices ?? []).length === 0 ? (
        <p className="text-sm text-ink-muted">
          {tr("还没有登记任何浏览器。", "No browser is enrolled.")}
        </p>
      ) : (
        <ul className="space-y-2">
          {(devices.data?.devices ?? []).map((d) => (
            <li key={d.id} className="flex items-center justify-between gap-3 text-sm">
              <span className="min-w-0 flex-1 truncate text-ink">{d.label}</span>
              <span className="shrink-0 text-xs text-ink-faint">
                {agoText((Date.now() - d.created_ms) / 1000)}
              </span>
              <Button onClick={() => revoke(d.id)}>{tr("撤销", "Revoke")}</Button>
            </li>
          ))}
        </ul>
      )}
      <p className="mt-3 text-xs leading-relaxed text-ink-faint">
        {tr(
          `一台登记过的浏览器只带设备凭证，不再要密码与验证码，有效期 ${days} 天。撤销立刻生效，那台浏览器下一次请求就会回到登录页。`,
          `An enrolled browser carries only a device credential — no password, no code — for ${days} days. Revoking takes effect at once: that browser is back at the sign-in page on its next request.`,
        )}
      </p>
      {/* The other way to get on this list, which starts on a command
          line and so has nowhere else to be found from. */}
      <p className="mt-2 text-xs leading-relaxed text-ink-faint">
        {trustedKeys ? (
          <>
            {tr("在没有密码的新机器上，在那台机器运行", "On a machine with no password yet, run")}{" "}
            <Code>scripts/deck-enrol.sh</Code>
            {/* The space leads the English string: Chinese wants none
                before the dash, English needs one. */}
            {tr("——它用", " — it signs with a key listed in")} <Code>{trustedKeys}</Code>{" "}
            {tr("里列出的密钥签名——再打开它打印的链接即可登记。", "— and opening the link it prints enrols the browser.")}
          </>
        ) : (
          tr(
            "想在没敲过密码的新机器上登记浏览器，用 OQ_DECK_TRUSTED_KEYS 指向一个 allowed_signers 文件，重启 deck 后登录页会多出一行说明。",
            "To enrol a browser on a machine you have never typed the password on, point OQ_DECK_TRUSTED_KEYS at an allowed_signers file; the sign-in page gains a line about it once the deck restarts.",
          )
        )}
      </p>
    </Card>
  );
}

export function Settings() {
  const s = useQuery({ queryKey: ["settings"], queryFn: api.settings });
  const caps = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { locale, setLocale } = useLocale();
  const { choice, setChoice } = useTheme();

  const logout = async () => {
    await api.logout().catch(() => undefined);
    await queryClient.invalidateQueries();
    navigate("/login", { replace: true });
  };

  const header = (
    <PageHeader
      title={tr("设置", "Settings")}
      description={tr(
        "deck 的配置来自服务的环境变量；这里只显示，改动要改变量后重启 deck。",
        "The deck is configured by its service's environment variables. They are shown here; to change one, set the variable and restart the deck.",
      )}
      actions={
        <Button icon={<LogOut className="h-4 w-4" />} onClick={() => void logout()}>
          {tr("登出", "Sign out")}
        </Button>
      }
    />
  );

  if (s.isLoading) {
    return (
      <div>
        {header}
        <Skeleton rows={10} />
      </div>
    );
  }
  if (s.isError) {
    return (
      <div>
        {header}
        <ErrorState error={s.error} what={tr("设置", "settings")} />
      </div>
    );
  }
  const d = s.data!;

  return (
    <div className="max-w-4xl">
      {header}
      <div className="space-y-5">
        <Card
          title={tr("写入模式", "Write mode")}
          icon={<PenLine className="h-4 w-4" />}
          tone={d.allow_writes ? "warn" : undefined}
          extra={d.allow_writes ? <Badge tone="warn">{tr("已开启", "On")}</Badge> : <Badge>{tr("只读", "Read-only")}</Badge>}
        >
          <p className="text-sm text-ink">{d.allow_writes
              ? tr("写入模式：可以停机、退出、启停服务、部署。", "Write mode: the deck can halt, shut down, start and stop services, and deploy.")
              : tr("只读模式：只能看，不能改任何东西。", "Read-only: the deck can look but cannot change anything.")}</p>
          <p className="mt-1.5 text-xs leading-relaxed text-ink-muted">
            {caps.data?.writes.available
              ? tr("高风险操作仍需主机代理验证的二次验证码。", "Risky operations still need a step-up code that the host agent verifies.")
              : caps.data?.writes.reason}{" "}
            {tr("改变模式：设置或去掉", "To change the mode, set or remove")} <Code>OQ_DECK_ALLOW_WRITES=1</Code>{" "}
            {tr("后重启 deck。", "and restart the deck.")}
          </p>
        </Card>

        <Card title={tr("数据来源", "Data sources")} icon={<Database className="h-4 w-4" />}>
          <KV
            items={[
              row(tr("运行记录目录", "Run records"), d.runs_dir, "OQ_DECK_RUNS_DIR"),
              row(tr("journal 目录", "Journals"), d.journals_dir, "OQ_DECK_JOURNALS_DIR"),
              row(tr("tick 目录", "Ticks"), d.ticks_dir, "OQ_DECK_TICKS_DIR"),
              row(tr("交易所最新记录", "Latest venue record"), d.venue_record, "OQ_DECK_VENUE_RECORD"),
              row(tr("主机代理", "Host agent"), d.agent_socket, "OQ_DECK_AGENT_SOCKET"),
              row(
                tr("上游版本检查", "Upstream release check"),
                d.upstream.every_hours > 0
                  ? tr(`${d.upstream.repo}，每 ${d.upstream.every_hours} 小时`, `${d.upstream.repo}, every ${d.upstream.every_hours} h`)
                  : tr("已关闭，不访问 GitHub", "Off; no request to GitHub"),
                "OQ_DECK_UPSTREAM_REPO / OQ_DECK_UPSTREAM_CHECK_HOURS",
              ),
              row(
                tr("本控制台的发布仓库", "This console's release repository"),
                d.upstream.every_hours > 0 ? d.upstream.self_repo : tr("已关闭，不访问 GitHub", "Off; no request to GitHub"),
                "OQ_DECK_SELF_REPO",
              ),
              row(tr("上游检查走的代理", "Proxy for the upstream check"), d.upstream.proxy ? tr("已设置", "Set") : tr("无，直连", "None; direct"), "OQ_DECK_UPSTREAM_PROXY"),
              row(tr("deck 构建所用的框架提交", "Framework commit the deck was built from"), d.upstream.framework_rev || null),
              row(
                tr("定期报告", "Scheduled reports"),
                d.reports.every_hours > 0 && d.reports.dir
                  ? tr(`${d.reports.dir}，每 ${d.reports.every_hours} 小时`, `${d.reports.dir}, every ${d.reports.every_hours} h`)
                  : null,
                "OQ_DECK_REPORTS_DIR / OQ_DECK_REPORT_HOURS",
              ),
            ]}
          />
        </Card>

        <Card title={tr("访问与会话", "Access and sessions")} icon={<LockKeyhole className="h-4 w-4" />}>
          <KV
            items={[
              row(tr("监听", "Listening on"), d.listen, "OQ_DECK_HOST / OQ_DECK_PORT"),
              row(tr("在 TLS 代理之后", "Behind a TLS proxy"), d.behind_tls ? tr("是", "Yes") : tr("否", "No"), "OQ_DECK_BEHIND_TLS"),
              row(tr("额外应答的名字", "Extra host names"), d.extra_hosts.length ? d.extra_hosts.join(", ") : tr("无", "None"), "OQ_DECK_EXTRA_HOSTS"),
              row(tr("第二因素（TOTP）", "Second factor (TOTP)"), d.totp ? tr("已启用", "On") : tr("未启用", "Off"), "OQ_DECK_TOTP_SECRET"),
              row(tr("空闲过期", "Idle timeout"), tr(`${d.session.idle_minutes} 分钟`, `${d.session.idle_minutes} min`)),
              row(tr("最长会话", "Longest session"), tr(`${d.session.absolute_hours} 小时`, `${d.session.absolute_hours} h`)),
            ]}
          />
          <p className="mt-3 text-xs text-ink-faint">{tr("deck 重启即全部会话失效。", "Restarting the deck ends every session.")}</p>
          <p className="mt-1.5 text-xs text-ink-faint">
            {tr(
              "第二因素说的是登录。停机、部署这类高风险操作要的是另一个码，由主机代理保管、deck 读不到：在主机上运行 sudo ~/agent-totp-qr.sh 取得。",
              "That second factor is the one for signing in. High-risk actions want a different code, which the host agent holds and this deck cannot read: sudo ~/agent-totp-qr.sh on the host shows it.",
            )}
          </p>
        </Card>

        <EnrolledBrowsers
          can={s.data?.devices ?? false}
          days={s.data?.device_days ?? 0}
          trustedKeys={s.data?.trusted_keys ?? null}
        />

        <Card title={tr("界面", "Interface")} icon={<Monitor className="h-4 w-4" />}>
          <KV
            items={[
              [
                tr("语言", "Language"),
                <Segmented<Locale>
                  value={locale}
                  onChange={setLocale}
                  options={[
                    { value: "zh", label: "中文" }, // i18n-ok
                    { value: "en", label: "English" },
                  ]}
                />,
              ],
              [
                tr("主题", "Theme"),
                <Segmented<ThemeChoice>
                  value={choice}
                  onChange={setChoice}
                  options={[
                    { value: "light", label: tr("浅色", "Light") },
                    { value: "dark", label: tr("深色", "Dark") },
                    { value: "system", label: tr("跟随系统", "System") },
                  ]}
                />,
              ],
            ]}
          />
          <p className="mt-3 flex items-center gap-1.5 text-xs text-ink-muted">
            <Info className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
            {tr(
              "术语旁的 ⓘ 或带虚线下划线的词，鼠标悬停即显示一句解释。",
              "Hover over an ⓘ or a word with a dotted underline for a one-line explanation.",
            )}
          </p>
        </Card>

        <Card title={tr("版本", "Version")} icon={<Info className="h-4 w-4" />}>
          <KV items={[[tr("deck 版本", "Deck version"), <span className="font-mono text-xs">{d.version}</span>]]} />
        </Card>
      </div>
    </div>
  );
}

/** A setting, the variable that changes it, and "not set" for none. */
function row(label: string, value: ReactNode, variable?: string): [ReactNode, ReactNode] {
  return [
    <span>
      {label}
      {variable && <span className="mt-0.5 block font-mono text-[11px] text-ink-faint">{variable}</span>}
    </span>,
    value == null ? (
      <span className="text-warn">{tr("未配置", "Not set")}</span>
    ) : (
      <span className="font-mono text-xs" title={typeof value === "string" ? value : undefined}>
        {value}
      </span>
    ),
  ];
}

function Code({ children }: { children: ReactNode }) {
  return <code className="rounded bg-surface-raised px-1 font-mono text-ink">{children}</code>;
}
