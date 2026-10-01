import { useState } from "react";
import { Link } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowUpRight, RefreshCw, Tag } from "lucide-react";

import { ApiError, api, type ConsoleRelease, type UpstreamReport, type UpstreamRevision } from "@/api/client";
import { intlLocale, pair, tr } from "@/i18n";
import { Ago, Badge, Card, cx, type Tone } from "@/ui/kit";

/**
 * Whether what runs here is behind the framework's newest release, and
 * whether this console is behind quanterdeck's own.
 *
 * The deck asks GitHub on a schedule and keeps the answer; this reads
 * the kept answer and never reaches GitHub itself. The rule it is drawn
 * by is the console's own: "cannot tell" — not checked yet, the check
 * failed, a revision GitHub does not know — is never drawn as "up to
 * date". The two checks are kept apart, so each section here shows its
 * own failure and its own age.
 */
export function useUpstream(enabled: boolean) {
  return useQuery({ queryKey: ["upstream"], queryFn: api.upstream, refetchInterval: 5 * 60_000, enabled });
}

const short = (sha: string | null | undefined) => (sha ? sha.slice(0, 7) : "—");

function whatLabel(r: UpstreamRevision): string {
  if (r.what === "deck") return tr("本控制台（deck）", "This console (deck)");
  return r.release ? tr(`交易进程（发布 ${r.release}）`, `Trader (release ${r.release})`) : tr("交易进程", "Trader");
}

/** A revision's standing, in words and a tone. */
function verdict(r: UpstreamRevision): { tone: Tone; text: string } {
  switch (r.verdict) {
    case "includes":
      return {
        tone: "good",
        text: r.ahead_by ? tr(`已包含该版本，另领先 ${r.ahead_by} 个提交`, `Includes the release, ${r.ahead_by} commits ahead`) : tr("已包含该版本", "Includes the release"),
      };
    case "behind":
      return { tone: "warn", text: tr(`落后 ${r.behind_by ?? "?"} 个提交，有新版本`, `${r.behind_by ?? "?"} commits behind: a newer release exists`) };
    case "diverged":
      return {
        tone: "warn",
        text: tr(`分叉：领先 ${r.ahead_by ?? "?"}、落后 ${r.behind_by ?? "?"} 个提交`, `Diverged: ${r.ahead_by ?? "?"} ahead, ${r.behind_by ?? "?"} behind`),
      };
    default:
      return { tone: "neutral", text: tr("无法判断", "Cannot tell") };
  }
}

/** The console's version against its newest release, in words and a tone. */
function standing(c: ConsoleRelease): { tone: Tone; text: string } {
  switch (c.verdict) {
    case "current":
      return { tone: "good", text: tr("已是最新发布", "The newest release") };
    case "ahead":
      return { tone: "good", text: tr("比最新发布更新", "Newer than the newest release") };
    case "behind":
      return { tone: "warn", text: tr("有新版本", "A newer release exists") };
    default:
      return c.published === false ? { tone: "neutral", text: tr("尚无发布", "No release yet") } : { tone: "neutral", text: tr("无法判断", "Cannot tell") };
  }
}

/** Only the release's own page is linked; the server refuses others too. */
const releaseLink = (url: string) => (url.startsWith("https://github.com/") ? url : null);

function RefreshButton({ report }: { report: UpstreamReport }) {
  const client = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  async function refresh() {
    setBusy(true);
    setProblem(null);
    try {
      client.setQueryData(["upstream"], await api.upstreamRefresh());
    } catch (e) {
      setProblem(e instanceof ApiError ? e.detail : String(e));
    } finally {
      setBusy(false);
    }
  }
  const spinning = busy || report.checking;
  return (
    <span className="inline-flex items-center gap-2">
      {problem && <span className="max-w-56 truncate text-warn" title={problem}>{problem}</span>}
      <button
        onClick={refresh}
        disabled={spinning}
        title={tr("现在检查（每分钟最多一次）", "Check now (at most once a minute)")}
        className="inline-flex items-center gap-1 hover:text-ink disabled:opacity-50"
      >
        <RefreshCw className={cx("h-3.5 w-3.5", spinning && "animate-spin")} />
        {tr("检查", "Check")}
      </button>
    </span>
  );
}

/** The overview's card. */
export function UpstreamCard({ available }: { available: boolean }) {
  const q = useUpstream(available);
  const r = q.data;
  return (
    <Card
      title={tr("上游版本", "Upstream release")}
      icon={<Tag />}
      hue="purple"
      tone={r?.behind || r?.console?.behind ? "warn" : undefined}
      extra={r?.enabled ? <RefreshButton report={r} /> : undefined}
      className="h-full"
    >
      {q.isError ? (
        <p className="text-sm text-warn">{q.error instanceof ApiError ? q.error.detail : String(q.error)}</p>
      ) : !r ? (
        <p className="text-sm text-ink-faint">{tr("读取中…", "Loading…")}</p>
      ) : !r.enabled ? (
        <p className="text-sm text-ink-muted">{pair(r.reason, r.reason_en)}</p>
      ) : (
        <UpstreamBody r={r} />
      )}
    </Card>
  );
}

function UpstreamBody({ r }: { r: UpstreamReport }) {
  const stale = r.error !== null && r.succeeded_at_ms !== null;
  return (
    <div className="space-y-4 text-sm">
      {r.checked_at_ms === null && (
        <p className="text-ink-muted">
          {r.checking ? tr("正在检查…", "Checking…") : tr("尚未检查。启动后不久会检查一次，之后按计划进行。", "Not checked yet. The first check runs shortly after startup, then on schedule.")}
        </p>
      )}
      {r.error !== null && (
        <div className="rounded-2xl border border-warn/40 bg-warn/8 px-3 py-2.5">
          <div className="font-medium text-ink">
            {tr("最近一次框架发布检查没有成功", "The last check of the framework release did not succeed")}
            {r.checked_at_ms !== null && (
              <span className="ml-1 font-normal text-ink-faint">
                (<Ago ms={r.checked_at_ms} />)
              </span>
            )}
          </div>
          <div className="mt-1 break-words text-xs text-ink-muted">{pair(r.error, r.error_en)}</div>
          {stale ? (
            <div className="mt-1 text-xs text-ink-faint">
              {tr("下面是上一次成功检查的结果，", "Below is the last successful check, from ")}
              <Ago ms={r.succeeded_at_ms!} />
              {tr("，可能已过时。", "; it may be out of date.")}
            </div>
          ) : (
            <div className="mt-1 text-xs text-ink-faint">{tr("还没有成功检查过，所以无法判断是否落后。", "No check has succeeded yet, so whether anything is behind cannot be told.")}</div>
          )}
        </div>
      )}
      {r.published === false && <p className="text-ink-muted">{tr(`${r.repo} 还没有发布任何版本。`, `${r.repo} has published no release yet.`)}</p>}
      {r.latest && (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span className="font-mono text-lg font-medium text-ink">{r.latest.tag}</span>
          {r.latest.name && r.latest.name !== r.latest.tag && <span className="text-ink-muted">{r.latest.name}</span>}
          {r.latest.prerelease && <Badge tone="warn">{tr("预发布", "Pre-release")}</Badge>}
          <span className="text-xs text-ink-faint">
            {r.latest.published_at && new Date(r.latest.published_at).toLocaleDateString(intlLocale())} · <span className="font-mono">{short(r.latest.sha)}</span>
          </span>
          <a href={r.latest.url} target="_blank" rel="noopener noreferrer" className="ml-auto inline-flex items-center gap-1 text-xs text-accent hover:underline">
            {tr("发布说明", "Release notes")} <ArrowUpRight className="h-3.5 w-3.5" />
          </a>
        </div>
      )}
      {r.revisions.length > 0 && (
        <ul className="space-y-2">
          {r.revisions.map((rev) => {
            const v = verdict(rev);
            const why = pair(rev.reason, rev.reason_en);
            return (
              <li key={rev.what} className="rounded-2xl border border-line px-3 py-2.5">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-ink">{whatLabel(rev)}</span>
                  <span className="font-mono text-xs text-ink-faint">{short(rev.rev)}</span>
                  <span className="ml-auto">
                    <Badge tone={v.tone} dot>
                      {v.text}
                    </Badge>
                  </span>
                </div>
                {rev.verdict === "unknown" && why && <div className="mt-1 text-xs text-ink-muted">{why}</div>}
              </li>
            );
          })}
        </ul>
      )}
      {r.console && <ConsoleSection c={r.console} />}
      <div className="text-xs text-ink-faint">
        {tr(`${r.repo} · 每 ${r.every_hours} 小时检查一次`, `${r.repo} · checked every ${r.every_hours} h`)}
        {r.succeeded_at_ms !== null && !stale && (
          <>
            {tr(" · 上次检查 ", " · last checked ")}
            <Ago ms={r.succeeded_at_ms} />
          </>
        )}
      </div>
    </div>
  );
}

/**
 * This console against quanterdeck's newest release: by version, since
 * a build from an archive has no commit to compare. Its own error and
 * age, independent of the framework section above.
 */
function ConsoleSection({ c }: { c: ConsoleRelease }) {
  if (c.checked_at_ms === null) return null;
  const stale = c.error !== null && c.succeeded_at_ms !== null;
  const v = standing(c);
  const why = pair(c.reason, c.reason_en);
  const link = c.latest ? releaseLink(c.latest.url) : null;
  return (
    <div className="space-y-2 border-t border-line pt-3">
      <div className="text-xs font-medium text-ink-faint">{tr("本控制台（quanterdeck 发布）", "This console (quanterdeck release)")}</div>
      {c.error !== null && (
        <div className="rounded-2xl border border-warn/40 bg-warn/8 px-3 py-2.5">
          <div className="font-medium text-ink">
            {tr("最近一次检查没有成功", "The last check did not succeed")}
            <span className="ml-1 font-normal text-ink-faint">
              (<Ago ms={c.checked_at_ms} />)
            </span>
          </div>
          <div className="mt-1 break-words text-xs text-ink-muted">{pair(c.error, c.error_en)}</div>
          {stale ? (
            <div className="mt-1 text-xs text-ink-faint">
              {tr("下面是上一次成功检查的结果，", "Below is the last successful check, from ")}
              <Ago ms={c.succeeded_at_ms!} />
              {tr("，可能已过时。", "; it may be out of date.")}
            </div>
          ) : (
            <div className="mt-1 text-xs text-ink-faint">{tr("还没有成功检查过，所以无法判断是否落后。", "No check has succeeded yet, so whether it is behind cannot be told.")}</div>
          )}
        </div>
      )}
      {c.succeeded_at_ms !== null && (
        <div className="rounded-2xl border border-line px-3 py-2.5">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-ink">{tr("运行中", "Running")}</span>
            <span className="font-mono text-xs text-ink">v{c.version}</span>
            {c.latest && (
              <>
                <span className="text-ink-faint">·</span>
                <span className="text-ink-muted">{tr("最新发布", "Newest release")}</span>
                <span className="font-mono text-xs text-ink">{c.latest.tag}</span>
                {c.latest.prerelease && <Badge tone="warn">{tr("预发布", "Pre-release")}</Badge>}
              </>
            )}
            <span className="ml-auto">
              <Badge tone={v.tone} dot>
                {v.text}
              </Badge>
            </span>
          </div>
          {c.verdict === "unknown" && why && <div className="mt-1 text-xs text-ink-muted">{why}</div>}
          {c.latest && (
            <div className="mt-1 flex flex-wrap items-center gap-x-3 text-xs text-ink-faint">
              {c.latest.published_at && <span>{new Date(c.latest.published_at).toLocaleDateString(intlLocale())}</span>}
              {link && (
                <a href={link} target="_blank" rel="noopener noreferrer" className="ml-auto inline-flex items-center gap-1 text-accent hover:underline">
                  {tr("发布说明", "Release notes")} <ArrowUpRight className="h-3.5 w-3.5" />
                </a>
              )}
            </div>
          )}
        </div>
      )}
      <div className="text-xs text-ink-faint">
        {c.repo}
        {c.succeeded_at_ms !== null && !stale && (
          <>
            {tr(" · 上次检查 ", " · last checked ")}
            <Ago ms={c.succeeded_at_ms} />
          </>
        )}
      </div>
    </div>
  );
}

/**
 * The header's notice: shown only when the last successful check found
 * something behind a release — the framework's, this console's, or
 * both, each named. A check that failed shows nothing here rather than
 * an all-clear — the card says what went wrong.
 */
export function UpstreamBadge({ available }: { available: boolean }) {
  const q = useUpstream(available);
  const r = q.data;
  if (!r) return null;
  const framework = r.behind && r.latest ? r.latest.tag : null;
  const own = r.console?.behind && r.console.latest ? r.console.latest.tag : null;
  if (!framework && !own) return null;
  return (
    <Link to="/" className="inline-flex items-center gap-1.5" title={tr("有更新的版本，见总览「上游版本」", "A newer release exists; see \"Upstream release\" on the overview")}>
      {own && (
        <Badge tone="warn" dot>
          {tr(`quanterdeck 新版本 ${own}`, `New quanterdeck release ${own}`)}
        </Badge>
      )}
      {framework && (
        <Badge tone="warn" dot>
          {tr(`新版本 ${framework}`, `New release ${framework}`)}
        </Badge>
      )}
    </Link>
  );
}
