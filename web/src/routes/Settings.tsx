import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { api } from "@/api/client";
import { ErrorState, ModeToggle, Skeleton } from "@/components/States";

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

  if (s.isLoading) return <Skeleton rows={10} />;
  if (s.isError) return <ErrorState error={s.error} what="设置" />;
  const d = s.data!;
  const row = (label: string, value: React.ReactNode, variable?: string) => (
    <tr className="border-t border-line align-top">
      <td className="w-48 py-2 text-ink-muted">{label}</td>
      <td className="py-2 font-mono text-xs">{value ?? <span className="text-warn">未配置</span>}</td>
      <td className="py-2 text-xs text-ink-muted">{variable}</td>
    </tr>
  );

  return (
    <div className="max-w-4xl space-y-6">
      <h1 className="text-lg text-ink">设置</h1>

      <section>
        <h2 className="mb-2 text-sm text-ink-muted">模式</h2>
        <div className={`rounded border p-3 text-sm ${d.allow_writes ? "border-warn bg-warn/10" : "border-line bg-surface"}`}>
          <p className="text-ink">{d.allow_writes ? "写入模式：可以停机、退出、启停服务、部署。" : "只读模式：只能看，不能改任何东西。"}</p>
          <p className="mt-1 text-xs text-ink-muted">
            {caps.data?.writes.available ? "高风险操作仍需主机代理验证的二次验证码。" : caps.data?.writes.reason}
            改变模式：设置或去掉 <code>OQ_DECK_ALLOW_WRITES=1</code> 后重启 deck。
          </p>
        </div>
      </section>

      <section>
        <h2 className="mb-2 text-sm text-ink-muted">界面</h2>
        <div className="flex items-center gap-3 text-sm">
          <span className="text-ink-muted">显示密度</span>
          <ModeToggle />
          <span className="text-xs text-ink-muted">新手模式隐藏高级字段、术语带一句解释；只保存在这个浏览器里。</span>
        </div>
        <p className="mt-2 text-xs text-ink-muted">语言：中文（界面文案目前只有中文）。主题：深色（按设计只有这一套）。</p>
      </section>

      <section>
        <h2 className="mb-2 text-sm text-ink-muted">数据来源</h2>
        <table className="w-full text-sm">
          <tbody>
            {row("运行记录目录", d.runs_dir, "OQ_DECK_RUNS_DIR")}
            {row("journal 目录", d.journals_dir, "OQ_DECK_JOURNALS_DIR")}
            {row("tick 目录", d.ticks_dir, "OQ_DECK_TICKS_DIR")}
            {row("交易所最新记录", d.venue_record, "OQ_DECK_VENUE_RECORD")}
            {row("主机代理", d.agent_socket, "OQ_DECK_AGENT_SOCKET")}
          </tbody>
        </table>
      </section>

      <section>
        <h2 className="mb-2 text-sm text-ink-muted">访问与会话</h2>
        <table className="w-full text-sm">
          <tbody>
            {row("监听", d.listen, "OQ_DECK_HOST / OQ_DECK_PORT")}
            {row("在 TLS 代理之后", d.behind_tls ? "是" : "否", "OQ_DECK_BEHIND_TLS")}
            {row("额外应答的名字", d.extra_hosts.length ? d.extra_hosts.join(", ") : "无", "OQ_DECK_EXTRA_HOSTS")}
            {row("第二因素（TOTP）", d.totp ? "已启用" : "未启用", "OQ_DECK_TOTP_SECRET")}
            {row("会话", `空闲 ${d.session.idle_minutes} 分钟、最长 ${d.session.absolute_hours} 小时过期；deck 重启即全部失效`)}
            {row("版本", d.version)}
          </tbody>
        </table>
        <button
          className="mt-3 rounded border border-line px-3 py-1 text-sm text-ink hover:bg-surface-raised"
          onClick={async () => {
            await api.logout().catch(() => undefined);
            await queryClient.invalidateQueries();
            navigate("/login", { replace: true });
          }}
        >
          登出
        </button>
      </section>
    </div>
  );
}
