import type { ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { Database, Info, LockKeyhole, LogOut, Monitor, PenLine } from "lucide-react";

import { api } from "@/api/client";
import { ErrorState, Skeleton } from "@/components/States";
import { tr, useLocale, type Locale } from "@/i18n";
import { Badge, Button, Card, KV, PageHeader, Segmented } from "@/ui/kit";
import { useTheme, type ThemeChoice } from "@/ui/theme";

/**
 * What this deck is, how it is reached, and what it may do (UI-BRIEF
 * §4.7). Configuration lives in the service's environment, so this page
 * shows it and says which variable changes it — a console that edited
 * its own security settings from inside a session would be one a stolen
 * session could loosen.
 */
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
        </Card>

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
