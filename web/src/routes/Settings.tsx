import type { ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { Database, Info, LockKeyhole, LogOut, Monitor, PenLine } from "lucide-react";

import { api } from "@/api/client";
import { ErrorState, Skeleton } from "@/components/States";
import { Badge, Button, Card, KV, PageHeader } from "@/ui/kit";

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

  const logout = async () => {
    await api.logout().catch(() => undefined);
    await queryClient.invalidateQueries();
    navigate("/login", { replace: true });
  };

  const header = (
    <PageHeader
      title="设置"
      description="deck 的配置来自服务的环境变量；这里只显示，改动要改变量后重启 deck。"
      actions={
        <Button icon={<LogOut className="h-4 w-4" />} onClick={() => void logout()}>
          登出
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
        <ErrorState error={s.error} what="设置" />
      </div>
    );
  }
  const d = s.data!;

  return (
    <div className="max-w-4xl">
      {header}
      <div className="space-y-5">
        <Card
          title="写入模式"
          icon={<PenLine className="h-4 w-4" />}
          tone={d.allow_writes ? "warn" : undefined}
          extra={d.allow_writes ? <Badge tone="warn">已开启</Badge> : <Badge>只读</Badge>}
        >
          <p className="text-sm text-ink">{d.allow_writes ? "写入模式：可以停机、退出、启停服务、部署。" : "只读模式：只能看，不能改任何东西。"}</p>
          <p className="mt-1.5 text-xs leading-relaxed text-ink-muted">
            {caps.data?.writes.available ? "高风险操作仍需主机代理验证的二次验证码。" : caps.data?.writes.reason}
            改变模式：设置或去掉 <Code>OQ_DECK_ALLOW_WRITES=1</Code> 后重启 deck。
          </p>
        </Card>

        <Card title="数据来源" icon={<Database className="h-4 w-4" />}>
          <KV
            items={[
              row("运行记录目录", d.runs_dir, "OQ_DECK_RUNS_DIR"),
              row("journal 目录", d.journals_dir, "OQ_DECK_JOURNALS_DIR"),
              row("tick 目录", d.ticks_dir, "OQ_DECK_TICKS_DIR"),
              row("交易所最新记录", d.venue_record, "OQ_DECK_VENUE_RECORD"),
              row("主机代理", d.agent_socket, "OQ_DECK_AGENT_SOCKET"),
            ]}
          />
        </Card>

        <Card title="访问与会话" icon={<LockKeyhole className="h-4 w-4" />}>
          <KV
            items={[
              row("监听", d.listen, "OQ_DECK_HOST / OQ_DECK_PORT"),
              row("在 TLS 代理之后", d.behind_tls ? "是" : "否", "OQ_DECK_BEHIND_TLS"),
              row("额外应答的名字", d.extra_hosts.length ? d.extra_hosts.join(", ") : "无", "OQ_DECK_EXTRA_HOSTS"),
              row("第二因素（TOTP）", d.totp ? "已启用" : "未启用", "OQ_DECK_TOTP_SECRET"),
              row("空闲过期", `${d.session.idle_minutes} 分钟`),
              row("最长会话", `${d.session.absolute_hours} 小时`),
            ]}
          />
          <p className="mt-3 text-xs text-ink-faint">deck 重启即全部会话失效。</p>
        </Card>

        <Card title="界面" icon={<Monitor className="h-4 w-4" />}>
          <KV
            items={[
              ["语言", "中文（界面文案目前只有中文）"],
              ["主题", "深色（按设计只有这一套）"],
            ]}
          />
          <p className="mt-3 flex items-center gap-1.5 text-xs text-ink-muted">
            <Info className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
            术语旁的 ⓘ 或带虚线下划线的词，鼠标悬停即显示一句解释。
          </p>
        </Card>

        <Card title="版本" icon={<Info className="h-4 w-4" />}>
          <KV items={[["deck 版本", <span className="font-mono text-xs">{d.version}</span>]]} />
        </Card>
      </div>
    </div>
  );
}

/** A setting, the variable that changes it, and "未配置" for none. */
function row(label: string, value: ReactNode, variable?: string): [ReactNode, ReactNode] {
  return [
    <span>
      {label}
      {variable && <span className="mt-0.5 block font-mono text-[11px] text-ink-faint">{variable}</span>}
    </span>,
    value == null ? (
      <span className="text-warn">未配置</span>
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
