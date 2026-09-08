/**
 * A route that exists so the shape of the app is visible before the
 * screen is built. Says which milestone it belongs to rather than
 * "coming soon", so a reader can tell whether it is late or simply not
 * due yet.
 */
export function Placeholder({
  title,
  milestone,
  note,
}: {
  title: string;
  milestone: string;
  note?: string;
}) {
  return (
    <div className="max-w-xl">
      <h1 className="text-lg text-ink">{title}</h1>
      <p className="mt-2 text-sm text-ink-muted">
        计划在 <span className="font-mono text-accent">{milestone}</span> 交付。
      </p>
      {note && <p className="mt-3 text-sm text-ink-muted">{note}</p>}
    </div>
  );
}
