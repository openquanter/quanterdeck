import { useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { ArrowUpRight, Package, Rocket } from "lucide-react";

import { useCaps } from "@/features/trading";
import { UpstreamCard, useUpstream } from "@/features/upstream";
import { tr } from "@/i18n";
import { Card, PageHeader, cx } from "@/ui/kit";

/** Release discovery and the installation paths supported by this deployment. */
export function Upgrade() {
  const caps = useCaps();
  const available = caps.data?.upstream.available === true;
  const q = useUpstream(available);
  const [params, setParams] = useSearchParams();
  const framework = params.get("component") === "framework";
  const [target, setTarget] = useState("x86_64-unknown-linux-gnu");
  const latest = q.data?.console.latest;
  // Commands contain only a validated GitHub release coordinate, never arbitrary response text.
  const coordinate = latest?.url.match(/^https:\/\/github\.com\/([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+)\/releases\/tag\/(v\d+\.\d+\.\d+)$/);
  const bundle = coordinate ? `quanterdeck-${coordinate[2]}-${target}` : null;
  const download = coordinate && bundle ? `https://github.com/${coordinate[1]}/releases/download/${coordinate[2]}/${bundle}.tar.gz` : null;

  return (
    <div className="space-y-5">
      <PageHeader title={tr("版本升级", "Upgrade")} description={tr("选择要升级的组件，查看最新发布与安装步骤。", "Choose the component to upgrade and review its latest release and installation steps.")} />
      <div className="flex flex-wrap gap-2" role="group" aria-label={tr("升级组件", "Component to upgrade")}>
        {[
          ["console", tr("Quanterdeck 控制台", "Quanterdeck console")],
          ["framework", tr("OpenQuanter 框架 / 交易进程", "OpenQuanter framework / trader")],
        ].map(([component, label]) => (
          <button key={component} onClick={() => setParams({ component })} aria-pressed={framework === (component === "framework")} className={cx("rounded-xl border px-4 py-2 text-sm", framework === (component === "framework") ? "border-good/40 bg-good/12 text-good" : "border-line text-ink-muted hover:text-ink")}>
            {label}
          </button>
        ))}
      </div>
      {framework ? (
        <Card title={tr("升级框架与交易进程", "Upgrade the framework and trader")} icon={<Rocket />} hue="green">
          <ol className="list-decimal space-y-3 pl-5 text-sm text-ink-muted">
            <li>{tr("查看下方框架发布说明和运行中的 commit，选择目标发布版本。", "Review the framework release notes and running commits below, then choose the target release.")}</li>
            <li>{tr("在你的交易发布仓将 OpenQuanter 依赖固定到目标 release commit，完成构建和验证，再运行 ops/release.sh 签名并上传交易构件。", "In your trader release repository, pin OpenQuanter to the target release commit, build and verify, then run ops/release.sh to sign and upload the trader artifact.")}</li>
            <li>{tr("进入「发布」部署已暂存且签名校验通过的构件。部署会先停止交易进程，健康检查失败时自动回滚。", "Open Releases to deploy a staged artifact with a verified signature. Deployment stops the trader first and automatically rolls back if the health check fails.")}</li>
          </ol>
          {caps.data?.ops.available ? (
            <Link to="/deploy" className="mt-4 inline-flex items-center gap-2 rounded-xl bg-good/12 px-4 py-2 text-sm font-medium text-good hover:bg-good/20">{tr("进入发布与部署", "Open releases and deployment")} <ArrowUpRight className="h-4 w-4" /></Link>
          ) : (
            <p className="mt-4 text-sm text-ink-muted">{tr("当前控制台未连接可用的主机代理；签名构件部署入口会在代理可用后出现。", "This console has no available host agent; signed-artifact deployment becomes accessible when the agent is available.")}</p>
          )}
        </Card>
      ) : (
        <Card title={tr("升级 Quanterdeck 控制台", "Upgrade the Quanterdeck console")} icon={<Package />} hue="green">
          <p className="text-sm text-ink-muted">{tr("控制台通过主机上的发布包升级。下载和校验后，使用当前部署的服务管理方式切换版本并重启控制台。", "Upgrade the console using a release bundle on its host. After downloading and verifying it, switch versions and restart the console using your deployment's service manager.")}</p>
          {download && bundle ? (
            <div className="mt-4 space-y-3">
              <label className="flex flex-wrap items-center gap-3 text-sm text-ink">
                {tr("控制台主机架构", "Console host architecture")}
                <select value={target} onChange={(e) => setTarget(e.target.value)} className="rounded-lg border border-line bg-surface px-3 py-2">
                  <option value="x86_64-unknown-linux-gnu">Linux x86_64</option>
                  <option value="aarch64-unknown-linux-gnu">Linux aarch64 / ARM64</option>
                </select>
              </label>
              <div className="flex flex-wrap gap-4 text-sm text-good">
                <a href={download} className="hover:underline">{tr(`下载 ${latest!.tag} 发布包`, `Download the ${latest!.tag} bundle`)}</a>
                <a href={`${download}.sha256`} className="hover:underline">{tr("下载 SHA-256 校验文件", "Download the SHA-256 checksum")}</a>
              </div>
              <p className="text-xs text-ink-faint">{tr("在控制台主机的新目录中执行：", "Run in a new directory on the console host:")}</p>
              <pre className="overflow-x-auto rounded-xl border border-line bg-surface p-3 text-xs text-ink">{`curl -fLO ${download}\ncurl -fLO ${download}.sha256\nsha256sum -c ${bundle}.tar.gz.sha256 && tar xzf ${bundle}.tar.gz`}</pre>
            </div>
          ) : (
            <p className="mt-4 text-sm text-ink-muted">{tr("成功检查到正式发布版本后，此处会显示对应架构的下载链接与校验命令。可在下方「上游版本」中查看状态并重新检查。", "After a successful check finds a stable release, download links and verification commands for each architecture appear here. Review the status and check again in Upstream release below.")}</p>
          )}
          <ol className="mt-4 list-decimal space-y-3 pl-5 text-sm text-ink-muted">
            <li>{tr("保留当前版本作为回退，同时保留现有环境配置、状态目录和认证数据。", "Keep the current version for rollback and preserve existing environment configuration, state directories and authentication data.")}</li>
            <li>{tr("将控制台服务的可执行文件指向新包的 bin/oq-deck，并将 OQ_DECK_WEB_DIST 指向同一包的 web-dist。通过现有服务管理器重启控制台。", "Point the console service executable to bin/oq-deck in the new bundle and OQ_DECK_WEB_DIST to web-dist in the same bundle. Restart the console through its existing service manager.")}</li>
            <li>{tr("重新打开控制台，核对下方运行版本及登录、代理连接是否正常；如有异常，切回保留的旧包。", "Reopen the console and verify the running version below, login and agent connectivity. If anything fails, switch back to the retained bundle.")}</li>
          </ol>
          <p className="mt-4 text-xs text-ink-faint">{tr("包内的 oq-agent 由主机服务单独管理；替换控制台不会自动更新代理或交易进程。", "The bundled oq-agent is managed by its own host service; replacing the console does not automatically update the agent or trader.")}</p>
        </Card>
      )}
      <UpstreamCard available={available} />
    </div>
  );
}
