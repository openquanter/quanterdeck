import { useQuery } from "@tanstack/react-query";

import { api } from "@/api/client";

export function Overview() {
  const { data: caps } = useQuery({
    queryKey: ["capabilities"],
    queryFn: api.capabilities,
  });
  const { data: services } = useQuery({
    queryKey: ["services"],
    queryFn: api.services,
    enabled: caps?.services === true,
  });

  const running = services?.filter((s) => s.status === "running").length ?? 0;

  return (
    <div>
      <h1 className="mb-4 text-lg text-ink">总览</h1>
      <div className="grid gap-3 sm:grid-cols-3">
        <Tile label="运行时" value={caps?.kind ?? "—"} sub={caps?.version} />
        <Tile
          label="服务在线"
          value={services ? `${running}/${services.length}` : "—"}
        />
        <Tile label="写入模式" value="只读" sub="在设置中开启写入" />
      </div>

      {caps && <Unavailable caps={caps} />}
    </div>
  );
}

/**
 * What this runtime cannot do, and why. Shown rather than hidden: an
 * operator who cannot find a feature should learn here that it does not
 * exist yet, instead of concluding the console is broken.
 */
function Unavailable({ caps }: { caps: { notes: Record<string, string> } }) {
  const notes = Object.entries(caps.notes);
  if (notes.length === 0) return null;
  return (
    <section className="mt-8">
      <h2 className="mb-2 text-sm text-ink-muted">本运行时暂不支持</h2>
      <ul className="space-y-1 text-sm">
        {notes.map(([key, why]) => (
          <li key={key} className="flex gap-3">
            <span className="w-32 shrink-0 font-mono text-xs text-ink-muted">
              {key}
            </span>
            <span className="text-ink-muted">{why}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

function Tile({
  label,
  value,
  sub,
}: {
  label: string;
  value: string;
  sub?: string;
}) {
  return (
    <div className="rounded border border-line bg-surface p-4">
      <div className="text-xs text-ink-muted">{label}</div>
      <div className="mt-1 text-xl text-ink">{value}</div>
      {sub && <div className="mt-1 text-xs text-ink-muted">{sub}</div>}
    </div>
  );
}
