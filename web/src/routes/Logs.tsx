import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Pause, Play, Search, X } from "lucide-react";

import { api } from "@/api/client";
import { ErrorState } from "@/components/States";
import { Button, Freshness, PageHeader, cx } from "@/ui/kit";

/** One-click filters for the lines an operator looks for first. */
const QUICK = ["HALT", "MISMATCH", "FAIL", "operator", "heartbeat"];

/** Lines that name trouble, tinted so they stand out in a long tail. */
const WARN = /HALT|MISMATCH|FAIL|REFUSED|UNAVAILABLE/;

const LINE_COUNTS = [100, 200, 500, 2000];

const control = "h-8 rounded-md border border-line-strong bg-surface-raised px-2.5 text-xs text-ink outline-none focus:border-accent";

/**
 * A process's output, filtered, refreshed while watched (docs/UI-V4
 * §4.5): from the systemd journal (every line timestamped) for each
 * managed service, or from a log file for what was written before output
 * went there. The page is the terminal; the controls sit above it.
 */
export function Logs() {
  const files = useQuery({ queryKey: ["ops", "logs"], queryFn: api.logs, refetchInterval: 60_000 });
  const units = useQuery({ queryKey: ["ops", "units"], queryFn: api.units, refetchInterval: 60_000 });
  // "j:<unit>" for a service's journal, "f:<name>" for a file.
  const [name, setName] = useState<string | null>(null);
  const [lines, setLines] = useState(200);
  const [grep, setGrep] = useState("");
  const [follow, setFollow] = useState(true);
  useEffect(() => {
    // The services' own output first; files only once it is known there
    // is none. Choosing on whichever answered first opened an old file.
    if (name || units.isPending) return;
    if (units.data && units.data.length > 0) setName(`j:${units.data[0].unit}`);
    else if (files.data && files.data.length > 0) setName(`f:${files.data[0].name}`);
  }, [files.data, units.data, units.isPending, name]);

  // Following means the newest line is in view: the pane is kept at its
  // end as lines arrive, and left where the reader put it when paused.
  const pane = useRef<HTMLPreElement>(null);
  const tail = useQuery({
    queryKey: ["ops", "log", name, lines, grep],
    queryFn: async () => {
      const source = name as string;
      if (source.startsWith("j:")) {
        const t = await api.journalLog(source.slice(2), null, null, lines, grep);
        return { name: t.unit, size: 0, truncated: false, lines: t.lines };
      }
      return api.log(source.slice(2), lines, grep);
    },
    enabled: name !== null,
    refetchInterval: follow ? 5_000 : false,
  });
  useEffect(() => {
    if (follow && pane.current) pane.current.scrollTop = pane.current.scrollHeight;
  }, [follow, tail.data]);

  const shown = tail.data?.lines ?? [];

  return (
    <div className="flex h-[calc(100vh-7rem)] flex-col">
      <PageHeader
        title="日志"
        description="各服务写进 systemd journal 的输出（每行带时间），以及更早写在日志文件里的内容。"
        meta={<Freshness at={tail.dataUpdatedAt} fetching={tail.isFetching} staleAfterS={follow ? 15 : 3600} onRefresh={() => tail.refetch()} />}
      />
      {files.isError && (
        <div className="mb-3">
          <ErrorState error={files.error} what="日志文件列表" />
        </div>
      )}
      {units.isError && (
        <div className="mb-3">
          <ErrorState error={units.error} what="服务列表" />
        </div>
      )}

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <select className={cx(control, "font-mono")} value={name ?? ""} onChange={(e) => setName(e.target.value)}>
          {units.data && units.data.length > 0 && (
            <optgroup label="服务输出（systemd journal）">
              {units.data.map((u) => (
                <option key={u.unit} value={`j:${u.unit}`}>
                  {u.unit}
                </option>
              ))}
            </optgroup>
          )}
          {files.data && files.data.length > 0 && (
            <optgroup label="日志文件">
              {files.data.map((f) => (
                <option key={f.name} value={`f:${f.name}`}>
                  {f.name} ({(f.size / 1024).toFixed(0)} KiB)
                </option>
              ))}
            </optgroup>
          )}
        </select>

        <div className="relative">
          <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-ink-faint" />
          <input className={cx(control, "w-48 pl-7 pr-7")} placeholder="只看包含…" value={grep} onChange={(e) => setGrep(e.target.value)} />
          {grep && (
            <button className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-ink-faint hover:text-ink" onClick={() => setGrep("")} aria-label="清除过滤">
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </div>

        <div className="flex flex-wrap gap-1">
          {QUICK.map((k) => (
            <button
              key={k}
              className={cx(
                "h-7 rounded-full px-2.5 font-mono text-xs ring-1 ring-inset transition-colors",
                grep === k ? "bg-accent/12 text-accent ring-accent/40" : "text-ink-muted ring-line-strong hover:text-ink",
              )}
              onClick={() => setGrep(grep === k ? "" : k)}
            >
              {k}
            </button>
          ))}
        </div>

        <div className="ml-auto flex items-center gap-2">
          <select className={control} value={lines} onChange={(e) => setLines(Number(e.target.value))}>
            {LINE_COUNTS.map((n) => (
              <option key={n} value={n}>
                最后 {n} 行
              </option>
            ))}
          </select>
          <Button
            variant={follow ? "primary" : "secondary"}
            icon={follow ? <Pause className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
            onClick={() => setFollow(!follow)}
            title={follow ? "每 5 秒刷新；点击暂停" : "已暂停；点击恢复每 5 秒刷新"}
          >
            {follow ? "跟随中" : "已暂停"}
          </Button>
        </div>
      </div>

      {tail.isError ? (
        <ErrorState error={tail.error} what="日志内容" />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-[var(--radius-card)] border border-line bg-[#07090c]">
          <div className="flex items-center gap-3 border-b border-line px-3 py-1.5 font-mono text-[11px] text-ink-faint">
            <span className="truncate">{tail.data?.name ?? name?.slice(2) ?? "—"}</span>
            <span>{shown.length} 行</span>
            {grep && <span>过滤「{grep}」</span>}
            {tail.data?.truncated && <span className="text-warn">只读取了文件最后 4 MiB</span>}
          </div>
          {/* Rendered as text, never as markup: these lines come from a
              venue and a strategy, and are not ours to trust. */}
          <pre ref={pane} className="flex-1 overflow-auto p-3 font-mono text-[12px] leading-5 text-ink">
            {tail.isLoading ? (
              <span className="text-ink-faint">读取中…</span>
            ) : shown.length === 0 ? (
              <span className="text-ink-faint">{grep ? `没有包含「${grep}」的行。` : "这个来源还没有输出。"}</span>
            ) : (
              shown.map((line, i) => (
                <div key={i} className={WARN.test(line) ? "bg-warn/8 text-warn" : undefined}>
                  {highlight(line, grep)}
                </div>
              ))
            )}
          </pre>
        </div>
      )}
    </div>
  );
}

/** The filter's matches marked in the line, as text. */
function highlight(line: string, needle: string) {
  if (!needle) return line;
  const parts = line.split(needle);
  return parts.flatMap((p, i) => (i === 0 ? [p] : [<mark key={i} className="rounded-sm bg-accent/30 text-ink">{needle}</mark>, p]));
}
