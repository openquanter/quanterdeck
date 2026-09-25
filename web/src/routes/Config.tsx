import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { FileCog, GitCompare, History, RotateCcw, Save, Undo2 } from "lucide-react";

import { api, type OpsAction } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Empty, ErrorState, Skeleton } from "@/components/States";
import { useCaps } from "@/features/trading";
import { tr } from "@/i18n";
import { Ago, Badge, Button, Card, PageHeader, Segmented, Table, cx, fmtBytes, fmtTime } from "@/ui/kit";

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

type PendingAct = { title: string; consequence: string; action: OpsAction };

/**
 * Strategy configuration (docs/UI-V4 §3 变更): the files on the left, the
 * chosen one on the right as a form or as raw JSON, the difference shown
 * before anything is saved, the previous version kept automatically, and
 * any backup one step from being put back. The host agent does the
 * writing — refusing a file that moved since it was read (the sha sent
 * with every write) — and every change lands in the audit trail.
 */
export function Config() {
  const caps = useCaps();
  const writable = caps.data?.writes.available === true;
  const files = useQuery({ queryKey: ["ops", "configs"], queryFn: api.configs });
  const [name, setName] = useState<string | null>(null);
  useEffect(() => {
    if (!name && files.data?.length) setName(files.data[0].name);
  }, [files.data, name]);

  return (
    <div>
      <PageHeader
        title={tr("配置", "Config")}
        description={tr(
          "策略配置文件：保存前先看差异，旧版本自动备份，任一备份都可以回滚。交易进程重启后才会用新配置。",
          "Strategy config files: review the diff before saving, the old version is backed up automatically, and any backup can be rolled back to. The trading process uses a new config only after it restarts.",
        )}
        meta={!writable && caps.data ? <Badge tone="neutral">{tr("只读：写入模式未开启", "Read-only: write mode is off")}</Badge> : undefined}
      />
      {files.isLoading ? (
        <Skeleton rows={6} />
      ) : files.isError ? (
        <ErrorState error={files.error} what={tr("配置文件列表", "config file list")} />
      ) : !files.data?.length ? (
        <Empty
          title={tr("主机代理管理的配置目录里还没有文件。", "The config directory the host agent manages has no files yet.")}
          next={tr(
            "策略配置放在 /var/lib/oq/config/（由 provision.sh 迁移），之后在这里查看和修改。",
            "Strategy configs live in /var/lib/oq/config/ (migrated by provision.sh); view and edit them here afterwards.",
          )}
        />
      ) : (
        <div className="grid gap-5 lg:grid-cols-[16rem_1fr]">
          <Card title={tr("文件", "Files")} icon={<FileCog className="h-4 w-4" />} bodyClassName="p-1.5" className="self-start">
            <ul className="space-y-0.5">
              {files.data.map((f) => (
                <li key={f.name}>
                  <button
                    onClick={() => setName(f.name)}
                    className={cx(
                      "flex w-full flex-col rounded-md px-2.5 py-2 text-left transition-colors",
                      name === f.name ? "bg-accent/12 text-ink ring-1 ring-inset ring-accent/25" : "text-ink-muted hover:bg-surface-hover hover:text-ink",
                    )}
                  >
                    <span className="truncate font-mono text-xs">{f.name}</span>
                    <span className="mt-0.5 text-[11px] text-ink-faint">
                      {fmtBytes(f.size)} · {f.sha.slice(0, 8)}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </Card>
          <div className="min-w-0">{name && <Editor key={name} name={name} writable={writable} />}</div>
        </div>
      )}
    </div>
  );
}

function pretty(text: string) {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

function Editor({ name, writable }: { name: string; writable: boolean }) {
  const doc = useQuery({ queryKey: ["ops", "config", name], queryFn: () => api.config(name) });
  const [view, setView] = useState<"form" | "json">("form");
  const [raw, setRaw] = useState<string | null>(null);
  const [showDiff, setShowDiff] = useState(false);
  const [backup, setBackup] = useState<string | null>(null);
  const [pending, setPending] = useState<PendingAct | null>(null);

  useEffect(() => {
    if (doc.data && raw === null) setRaw(pretty(doc.data.content));
  }, [doc.data, raw]);

  const parsed = useMemo(() => {
    try {
      return { value: JSON.parse(raw ?? "null") as Json, error: null };
    } catch (e) {
      return { value: null, error: String(e) };
    }
  }, [raw]);

  const backupDoc = useQuery({
    queryKey: ["ops", "config", name, backup],
    queryFn: () => api.config(name, backup as string),
    enabled: backup !== null,
  });

  if (doc.isError) return <ErrorState error={doc.error} what={name} />;
  if (doc.isLoading || raw === null) return <Skeleton rows={10} />;
  const d = doc.data!;
  const original = pretty(d.content);
  const changed = raw !== original;

  return (
    <div className="space-y-5">
      <Card
        title={<span className="font-mono">{name}</span>}
        extra={
          <>
            {changed && <Badge tone="warn">{tr("未保存的修改", "Unsaved changes")}</Badge>}
            <span className="font-mono" title={d.sha}>
              {tr(`版本 ${d.sha.slice(0, 12)}`, `Version ${d.sha.slice(0, 12)}`)}
            </span>
            <Segmented
              value={view}
              onChange={setView}
              options={[
                { value: "form", label: tr("表单", "Form") },
                { value: "json", label: tr("原始 JSON", "Raw JSON") },
              ]}
            />
          </>
        }
        bodyClassName="p-0"
      >
        {view === "form" ? (
          parsed.error ? (
            <p className="px-4 py-3 text-sm text-warn">
              {tr("JSON 有错，先在「原始 JSON」里改好：", "The JSON has an error; fix it in Raw JSON first: ")}
              {parsed.error}
            </p>
          ) : (
            <div className="max-h-[32rem] overflow-auto px-4 py-3">
              <Field value={parsed.value} path={[]} onChange={(v) => setRaw(JSON.stringify(v, null, 2))} />
            </div>
          )
        ) : (
          <>
            <textarea
              className="block h-[28rem] w-full resize-y bg-ground p-3 font-mono text-xs leading-5 text-ink outline-none"
              value={raw}
              spellCheck={false}
              onChange={(e) => setRaw(e.target.value)}
            />
            {parsed.error && <p className="border-t border-line px-4 py-2 text-xs text-warn">{parsed.error}</p>}
          </>
        )}

        <div className="flex flex-wrap items-center gap-2 border-t border-line px-4 py-3">
          <Button size="sm" icon={<GitCompare className="h-3.5 w-3.5" />} disabled={!changed} onClick={() => setShowDiff(!showDiff)}>
            {showDiff ? tr("收起差异", "Hide diff") : tr("查看差异", "Show diff")}
          </Button>
          <Button size="sm" variant="ghost" icon={<Undo2 className="h-3.5 w-3.5" />} disabled={!changed} onClick={() => setRaw(original)}>
            {tr("放弃修改", "Discard changes")}
          </Button>
          <span className="text-xs text-ink-faint">{changed ? tr("保存前先看一眼差异。", "Check the diff before saving.") : tr("没有修改。", "No changes.")}</span>
          {writable && (
            <div className="ml-auto">
              <Button
                size="sm"
                variant="danger"
                icon={<Save className="h-3.5 w-3.5" />}
                disabled={!changed || Boolean(parsed.error)}
                onClick={() =>
                  setPending({
                    title: tr(`保存 ${name}`, `Save ${name}`),
                    consequence: tr(
                      "旧版本会自动备份。交易进程重启后才会用新配置；用这份配置的上线门控实例会因证据作废回到草稿。",
                      "The old version is backed up automatically. The trading process uses the new config only after it restarts; go-live gate instances using this config lose their evidence and return to draft.",
                    ),
                    action: { action: "config_put", name, content: raw, base_sha: d.sha },
                  })
                }
              >
                {tr("保存…", "Save…")}
              </Button>
            </div>
          )}
        </div>
        {changed && showDiff && (
          <div className="border-t border-line p-4">
            <DiffLegend before={tr("当前文件", "Current file")} after={tr("你的修改", "Your changes")} />
            <Diff before={original} after={raw} />
          </div>
        )}
      </Card>

      <Card title={tr(`历史版本（${d.backups.length}）`, `History (${d.backups.length})`)} icon={<History className="h-4 w-4" />} bodyClassName="p-0">
        {d.backups.length === 0 ? (
          <p className="px-4 py-4 text-sm text-ink-muted">
            {tr("还没有改过。每次保存前的版本都会留在这里。", "Never changed. The version before each save is kept here.")}
          </p>
        ) : (
          <Table head={[tr("备份时间", "Backed up"), tr("备份编号", "Backup ID"), ""]}>
            {d.backups.map((b) => (
              <tr key={b.id} className={backup === b.id ? "bg-accent/5" : undefined}>
                <td className="whitespace-nowrap text-ink">{b.at_ms ? <><Ago ms={b.at_ms} /><span className="ml-2 text-xs text-ink-faint">{fmtTime(b.at_ms)}</span></> : "—"}</td>
                <td className="font-mono text-xs text-ink-muted">{b.id}</td>
                <td className="text-right">
                  <div className="inline-flex gap-2">
                    <Button size="sm" variant="ghost" icon={<GitCompare className="h-3.5 w-3.5" />} onClick={() => setBackup(backup === b.id ? null : b.id)}>
                      {backup === b.id ? tr("收起", "Hide") : tr("与当前比较", "Compare with current")}
                    </Button>
                    {writable && (
                      <Button
                        size="sm"
                        variant="danger"
                        icon={<RotateCcw className="h-3.5 w-3.5" />}
                        onClick={() =>
                          setPending({
                            title: tr(`把 ${name} 回滚到 ${b.id}`, `Roll ${name} back to ${b.id}`),
                            consequence: tr(
                              "当前版本同样会先备份。交易进程重启后生效。",
                              "The current version is backed up first as well. Takes effect after the trading process restarts.",
                            ),
                            action: { action: "config_rollback", name, backup: b.id, base_sha: d.sha },
                          })
                        }
                      >
                        {tr("回滚到这个版本…", "Roll back to this version…")}
                      </Button>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </Table>
        )}
        {backup && (
          <div className="border-t border-line p-4">
            {backupDoc.isLoading ? (
              <Skeleton rows={4} />
            ) : backupDoc.isError ? (
              <ErrorState error={backupDoc.error} what={tr(`备份 ${backup}`, `backup ${backup}`)} />
            ) : backupDoc.data?.backup_content != null ? (
              <>
                <DiffLegend before={tr(`备份 ${backup}`, `Backup ${backup}`)} after={tr("当前文件", "Current file")} />
                <Diff before={pretty(backupDoc.data.backup_content)} after={original} />
              </>
            ) : null}
          </div>
        )}
      </Card>

      {pending && (
        <ActionDialog
          {...pending}
          highRisk
          onClose={() => {
            setPending(null);
            setRaw(null);
            void doc.refetch();
          }}
        />
      )}
    </div>
  );
}

/** A form from the JSON's own shape: numbers, strings and switches, nested. */
function Field({ value, path, onChange }: { value: Json; path: string[]; onChange: (v: Json) => void }) {
  if (value !== null && typeof value === "object" && !Array.isArray(value)) {
    return (
      <div className={path.length ? "ml-3 border-l border-line pl-4" : ""}>
        {Object.entries(value).map(([k, v]) => (
          <div key={k} className="py-1">
            <div className="flex items-center gap-3">
              <label className="w-52 shrink-0 truncate font-mono text-xs text-ink-muted" title={[...path, k].join(".")}>
                {k}
              </label>
              {(v === null || typeof v !== "object") && <Leaf value={v} onChange={(nv) => onChange({ ...value, [k]: nv })} />}
            </div>
            {v !== null && typeof v === "object" && <Field value={v} path={[...path, k]} onChange={(nv) => onChange({ ...value, [k]: nv })} />}
          </div>
        ))}
      </div>
    );
  }
  // A list of plain values — a ladder of sizes, of gaps — is edited in
  // place, one small box per rung, and keeps each rung's type.
  if (Array.isArray(value) && value.every((x) => x === null || typeof x !== "object")) {
    return (
      <div className="ml-3 flex flex-wrap gap-1.5">
        {value.map((x, i) => (
          <label key={i} className="flex items-center gap-1 text-[10px] text-ink-faint">
            {i}
            <input
              className="h-7 w-20 rounded-md border border-line-strong bg-ground px-2 font-mono text-xs text-ink outline-none focus:border-accent"
              defaultValue={String(x)}
              onBlur={(e) => {
                const t = e.target.value.trim();
                const next = typeof x === "number" ? Number(t) : typeof x === "boolean" ? t === "true" : t;
                if (typeof x === "number" && !Number.isFinite(next)) return;
                onChange(value.map((y, j) => (j === i ? next : y)) as Json);
              }}
            />
          </label>
        ))}
      </div>
    );
  }
  if (Array.isArray(value)) {
    return (
      <textarea
        className="ml-3 w-full rounded-md border border-line-strong bg-ground p-1.5 font-mono text-xs text-ink outline-none focus:border-accent"
        rows={Math.min(8, value.length + 1)}
        defaultValue={JSON.stringify(value)}
        onBlur={(e) => {
          try {
            onChange(JSON.parse(e.target.value));
          } catch {
            /* left as typed; the JSON view shows the error */
          }
        }}
      />
    );
  }
  return <Leaf value={value} onChange={onChange} />;
}

function Leaf({ value, onChange }: { value: Json; onChange: (v: Json) => void }) {
  const cls = "h-7 rounded-md border border-line-strong bg-ground px-2 font-mono text-xs text-ink outline-none focus:border-accent";
  if (typeof value === "boolean") {
    return <input type="checkbox" className="h-4 w-4 accent-[var(--color-accent)]" checked={value} onChange={(e) => onChange(e.target.checked)} />;
  }
  if (typeof value === "number") {
    return (
      <input
        className={`${cls} w-40 tabular-nums`}
        defaultValue={String(value)}
        onBlur={(e) => {
          const n = Number(e.target.value);
          if (e.target.value.trim() !== "" && Number.isFinite(n)) onChange(n);
          else e.target.value = String(value);
        }}
      />
    );
  }
  return <input className={`${cls} w-72`} value={typeof value === "string" ? value : ""} onChange={(e) => onChange(e.target.value)} />;
}

function DiffLegend({ before, after }: { before: string; after: string }) {
  return (
    <div className="mb-2 flex gap-4 text-xs text-ink-muted">
      <span>
        <span className="mr-1 font-mono text-ink-faint line-through">−</span>
        {before}
      </span>
      <span>
        <span className="mr-1 font-mono text-accent">+</span>
        {after}
      </span>
    </div>
  );
}

/** Line diff by longest common subsequence: enough for a config file. */
function Diff({ before, after }: { before: string; after: string }) {
  const rows = useMemo(() => {
    const a = before.split("\n"),
      b = after.split("\n");
    const m = a.length,
      n = b.length;
    const t: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
    for (let i = m - 1; i >= 0; i--) for (let j = n - 1; j >= 0; j--) t[i][j] = a[i] === b[j] ? t[i + 1][j + 1] + 1 : Math.max(t[i + 1][j], t[i][j + 1]);
    const out: { sign: " " | "-" | "+"; text: string }[] = [];
    let i = 0,
      j = 0;
    while (i < m && j < n) {
      if (a[i] === b[j]) {
        out.push({ sign: " ", text: a[i] });
        i++;
        j++;
      } else if (t[i + 1][j] >= t[i][j + 1]) out.push({ sign: "-", text: a[i++] });
      else out.push({ sign: "+", text: b[j++] });
    }
    while (i < m) out.push({ sign: "-", text: a[i++] });
    while (j < n) out.push({ sign: "+", text: b[j++] });
    return out;
  }, [before, after]);
  // Removed and added lines are told apart by sign and weight, not red and
  // green: a change is neither good nor bad (UI-BRIEF §8).
  return (
    <pre className="max-h-80 overflow-auto rounded-md border border-line bg-ground py-2 font-mono text-xs leading-5">
      {rows.map((r, k) => (
        <div key={k} className={cx("px-3", r.sign === "-" ? "bg-surface-raised text-ink-muted line-through" : r.sign === "+" ? "bg-accent/15 text-ink" : "text-ink-faint")}>
          {r.sign} {r.text}
        </div>
      ))}
    </pre>
  );
}
