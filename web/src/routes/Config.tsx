import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { api, type OpsAction } from "@/api/client";
import { ActionDialog } from "@/components/ActionDialog";
import { Empty, ErrorState, Skeleton } from "@/components/States";

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

/**
 * Strategy configuration (blueprint §6 P0 4): a form and the raw JSON,
 * the difference shown before anything is saved, the previous version
 * kept automatically, and any backup one step from being put back. The
 * host agent does the writing — refusing a file that moved since it was
 * read — and every change is in the audit trail.
 */
export function Config() {
  const caps = useQuery({ queryKey: ["capabilities"], queryFn: api.capabilities });
  const writable = caps.data?.writes.available === true;
  const files = useQuery({ queryKey: ["ops", "configs"], queryFn: api.configs });
  const [name, setName] = useState<string | null>(null);
  useEffect(() => {
    if (!name && files.data?.length) setName(files.data[0].name);
  }, [files.data, name]);

  return (
    <div className="space-y-4">
      <h1 className="text-lg text-ink">配置中心</h1>
      {files.isLoading ? (
        <Skeleton rows={6} />
      ) : files.isError ? (
        <ErrorState error={files.error} what="配置文件列表" />
      ) : !files.data?.length ? (
        <Empty title="主机代理管理的配置目录里还没有文件。" next="策略配置放在 /var/lib/oq/config/（由 provision.sh 迁移），之后在这里查看和修改。" />
      ) : (
        <div className="flex gap-4">
          <ul className="w-56 shrink-0 space-y-1 text-sm">
            {files.data.map((f) => (
              <li key={f.name}>
                <button
                  onClick={() => setName(f.name)}
                  className={`w-full rounded px-2 py-1 text-left font-mono text-xs ${name === f.name ? "bg-surface-raised text-ink" : "text-ink-muted hover:text-ink"}`}
                >
                  {f.name}
                </button>
              </li>
            ))}
          </ul>
          <div className="min-w-0 flex-1">{name && <Editor key={name} name={name} writable={writable} />}</div>
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
  const [pending, setPending] = useState<{ title: string; consequence: string; action: OpsAction } | null>(null);

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

  if (doc.isLoading || raw === null) return <Skeleton rows={10} />;
  if (doc.isError) return <ErrorState error={doc.error} what={name} />;
  const d = doc.data!;
  const original = pretty(d.content);
  const changed = raw !== original;

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2 text-xs">
        {(["form", "json"] as const).map((v) => (
          <button key={v} onClick={() => setView(v)} className={`rounded border px-2 py-1 ${view === v ? "border-accent text-accent" : "border-line text-ink-muted"}`}>
            {v === "form" ? "表单" : "原始 JSON"}
          </button>
        ))}
        <span className="ml-auto font-mono text-ink-muted">版本 {d.sha.slice(0, 12)}</span>
      </div>

      {view === "form" ? (
        parsed.error ? (
          <p className="text-sm text-warn">JSON 有错，先在「原始 JSON」里改好：{parsed.error}</p>
        ) : (
          <div className="rounded border border-line bg-surface p-3">
            <Field value={parsed.value} path={[]} onChange={(v) => setRaw(JSON.stringify(v, null, 2))} />
          </div>
        )
      ) : (
        <textarea
          className="h-96 w-full rounded border border-line bg-ground p-2 font-mono text-xs"
          value={raw}
          spellCheck={false}
          onChange={(e) => setRaw(e.target.value)}
        />
      )}
      {parsed.error && view === "json" && <p className="text-xs text-warn">{parsed.error}</p>}

      <div className="flex gap-2">
        <button className="rounded border border-line px-3 py-1 text-xs text-ink disabled:opacity-40" disabled={!changed} onClick={() => setShowDiff(!showDiff)}>
          {showDiff ? "收起差异" : "查看差异"}
        </button>
        <button className="rounded border border-line px-3 py-1 text-xs text-ink-muted disabled:opacity-40" disabled={!changed} onClick={() => setRaw(original)}>
          放弃修改
        </button>
        {writable && (
          <button
            className="rounded border border-bad px-3 py-1 text-xs text-bad disabled:opacity-40"
            disabled={!changed || Boolean(parsed.error)}
            onClick={() =>
              setPending({
                title: `保存 ${name}`,
                consequence:
                  "旧版本会自动备份。交易进程重启后才会用新配置；用这份配置的上线门控实例会因证据作废回到草稿。",
                action: { action: "config_put", name, content: raw, base_sha: d.sha },
              })
            }
          >
            保存…
          </button>
        )}
      </div>
      {changed && showDiff && <Diff before={original} after={raw} />}

      <section>
        <h2 className="mb-1 text-sm text-ink-muted">历史版本（{d.backups.length}）</h2>
        {d.backups.length === 0 ? (
          <p className="text-xs text-ink-muted">还没有改过。每次保存前的版本都会留在这里。</p>
        ) : (
          <ul className="space-y-1 text-xs">
            {d.backups.map((b) => (
              <li key={b.id} className="flex items-center gap-3">
                <span className="font-mono text-ink-muted">{b.at_ms ? new Date(b.at_ms).toLocaleString("zh-CN", { hour12: false }) : b.id}</span>
                <button className="text-accent hover:underline" onClick={() => setBackup(backup === b.id ? null : b.id)}>
                  {backup === b.id ? "收起" : "与当前比较"}
                </button>
                {writable && (
                  <button
                    className="text-bad hover:underline"
                    onClick={() =>
                      setPending({
                        title: `把 ${name} 回滚到 ${b.id}`,
                        consequence: "当前版本同样会先备份。交易进程重启后生效。",
                        action: { action: "config_rollback", name, backup: b.id, base_sha: d.sha },
                      })
                    }
                  >
                    回滚到这个版本…
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
        {backup && backupDoc.data?.backup_content != null && <Diff before={pretty(backupDoc.data.backup_content)} after={original} />}
      </section>

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
      <div className={path.length ? "ml-4 border-l border-line pl-3" : ""}>
        {Object.entries(value).map(([k, v]) => (
          <div key={k} className="py-1">
            <div className="flex items-center gap-3">
              <label className="w-48 shrink-0 truncate font-mono text-xs text-ink-muted" title={[...path, k].join(".")}>
                {k}
              </label>
              {(v === null || typeof v !== "object") && (
                <Leaf value={v} onChange={(nv) => onChange({ ...value, [k]: nv })} />
              )}
            </div>
            {v !== null && typeof v === "object" && (
              <Field value={v} path={[...path, k]} onChange={(nv) => onChange({ ...value, [k]: nv })} />
            )}
          </div>
        ))}
      </div>
    );
  }
  if (Array.isArray(value)) {
    return (
      <textarea
        className="ml-4 w-full rounded border border-line bg-ground p-1 font-mono text-xs"
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
  const cls = "rounded border border-line bg-ground px-2 py-0.5 font-mono text-xs";
  if (typeof value === "boolean") {
    return <input type="checkbox" checked={value} onChange={(e) => onChange(e.target.checked)} />;
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

/** Line diff by longest common subsequence: enough for a config file. */
function Diff({ before, after }: { before: string; after: string }) {
  const rows = useMemo(() => {
    const a = before.split("\n"), b = after.split("\n");
    const m = a.length, n = b.length;
    const t: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
    for (let i = m - 1; i >= 0; i--) for (let j = n - 1; j >= 0; j--) t[i][j] = a[i] === b[j] ? t[i + 1][j + 1] + 1 : Math.max(t[i + 1][j], t[i][j + 1]);
    const out: { sign: " " | "-" | "+"; text: string }[] = [];
    let i = 0, j = 0;
    while (i < m && j < n) {
      if (a[i] === b[j]) { out.push({ sign: " ", text: a[i] }); i++; j++; }
      else if (t[i + 1][j] >= t[i][j + 1]) out.push({ sign: "-", text: a[i++] });
      else out.push({ sign: "+", text: b[j++] });
    }
    while (i < m) out.push({ sign: "-", text: a[i++] });
    while (j < n) out.push({ sign: "+", text: b[j++] });
    return out;
  }, [before, after]);
  return (
    <pre className="max-h-80 overflow-auto rounded border border-line bg-ground p-2 font-mono text-xs">
      {rows.map((r, k) => (
        <div key={k} className={r.sign === "-" ? "bg-surface-raised text-ink-muted line-through" : r.sign === "+" ? "bg-accent/15 text-ink" : "text-ink-muted"}>
          {r.sign} {r.text}
        </div>
      ))}
    </pre>
  );
}
