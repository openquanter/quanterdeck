import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { api, ApiError, type Service } from "@/api/client";

const STATUS_STYLE: Record<string, string> = {
  running: "text-good",
  stopped: "text-ink-muted",
  unknown: "text-warn",
};

export function Services() {
  const queryClient = useQueryClient();
  const { data, isLoading, error } = useQuery({
    queryKey: ["services"],
    queryFn: api.services,
    refetchInterval: 10_000,
  });
  const [pending, setPending] = useState<{
    service: string;
    action: string;
    consequence: string;
  } | null>(null);

  const act = useMutation({
    mutationFn: ({
      service,
      action,
      confirm,
    }: {
      service: string;
      action: string;
      confirm?: string;
    }) => api.serviceAction(service, action, confirm ?? ""),
    onSuccess: () => {
      setPending(null);
      queryClient.invalidateQueries({ queryKey: ["services"] });
    },
    onError: (err) => {
      // A 409 is not a failure: it is the server telling us the action
      // needs a typed confirmation, and handing over the sentence to
      // show. Render that rather than a generic error toast.
      if (err instanceof ApiError && err.status === 409) {
        const detail = err.detail as {
          confirmation_required: string;
          consequence: string;
        };
        setPending((current) =>
          current
            ? { ...current, consequence: detail.consequence }
            : null,
        );
      }
    },
  });

  if (isLoading) return <SkeletonRows />;
  if (error) return <ErrorPanel error={error} />;

  return (
    <div>
      <h1 className="mb-4 text-lg text-ink">服务</h1>
      <div className="overflow-hidden rounded border border-line">
        <table className="w-full text-sm">
          <thead className="bg-surface text-left text-xs text-ink-muted">
            <tr>
              <th className="px-3 py-2 font-normal">服务</th>
              <th className="px-3 py-2 font-normal">状态</th>
              <th className="px-3 py-2 font-normal">PID</th>
              <th className="px-3 py-2 font-normal text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {data?.map((service: Service) => (
              <tr key={service.name} className="border-t border-line">
                <td className="px-3 py-2 font-mono">{service.name}</td>
                <td
                  className={`px-3 py-2 ${STATUS_STYLE[service.status] ?? ""}`}
                >
                  {service.status}
                </td>
                <td className="px-3 py-2 font-mono text-ink-muted">
                  {service.pid ?? "—"}
                </td>
                <td className="px-3 py-2 text-right">
                  {/* Restart is the only action offered inline. Stop and
                      start live in the danger zone below, because on the
                      daemon they cancel every resting order. */}
                  <button
                    className="rounded border border-line px-2 py-1 text-xs text-ink hover:bg-surface-raised"
                    onClick={() =>
                      act.mutate({
                        service: service.name,
                        action: "restart",
                      })
                    }
                  >
                    重启
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <DangerZone
        services={data ?? []}
        onRequest={(service, action) =>
          setPending({ service, action, consequence: "" })
        }
      />

      {pending && (
        <ConfirmDialog
          pending={pending}
          onCancel={() => setPending(null)}
          onConfirm={(typed) =>
            act.mutate({
              service: pending.service,
              action: pending.action,
              confirm: typed,
            })
          }
        />
      )}
    </div>
  );
}

function DangerZone({
  services,
  onRequest,
}: {
  services: Service[];
  onRequest: (service: string, action: string) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <details
      className="mt-8 rounded border border-bad/40"
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary className="cursor-pointer px-3 py-2 text-sm text-bad">
        危险操作
      </summary>
      <div className="border-t border-bad/40 p-3 text-sm">
        <p className="mb-3 text-ink-muted">
          停止与启动不是重启。守护进程被停止时会撤销全部挂单并丢失订单认领；
          若只是想让它加载新配置，用上面的「重启」。
        </p>
        <div className="space-y-2">
          {services.map((service) => (
            <div key={service.name} className="flex items-center gap-2">
              <span className="w-36 font-mono text-xs">{service.name}</span>
              <button
                className="rounded border border-line px-2 py-1 text-xs"
                onClick={() => onRequest(service.name, "stop")}
              >
                停止
              </button>
              <button
                className="rounded border border-line px-2 py-1 text-xs"
                onClick={() => onRequest(service.name, "start")}
              >
                启动
              </button>
            </div>
          ))}
        </div>
      </div>
    </details>
  );
}

function ConfirmDialog({
  pending,
  onCancel,
  onConfirm,
}: {
  pending: { service: string; action: string; consequence: string };
  onCancel: () => void;
  onConfirm: (typed: string) => void;
}) {
  const [typed, setTyped] = useState("");
  return (
    <div className="fixed inset-0 grid place-items-center bg-black/60 p-4">
      <div className="w-full max-w-md rounded border border-line bg-surface p-4">
        <h2 className="text-sm text-ink">
          {pending.action} · {pending.service}
        </h2>
        {pending.consequence && (
          <p className="mt-3 rounded bg-bad/10 p-3 text-sm text-ink">
            {pending.consequence}
          </p>
        )}
        <label className="mt-4 block text-xs text-ink-muted">
          输入服务名 <span className="font-mono text-ink">{pending.service}</span> 以确认
        </label>
        <input
          autoFocus
          value={typed}
          onChange={(event) => setTyped(event.target.value)}
          className="mt-1 w-full rounded border border-line bg-ground px-2 py-1 font-mono text-sm text-ink"
        />
        <div className="mt-4 flex justify-end gap-2">
          <button className="px-3 py-1 text-sm text-ink-muted" onClick={onCancel}>
            取消
          </button>
          <button
            disabled={typed !== pending.service}
            className="rounded bg-bad px-3 py-1 text-sm text-white disabled:opacity-40"
            onClick={() => onConfirm(typed)}
          >
            执行
          </button>
        </div>
      </div>
    </div>
  );
}

function SkeletonRows() {
  return (
    <div className="space-y-2">
      {Array.from({ length: 6 }).map((_, index) => (
        <div key={index} className="h-8 animate-pulse rounded bg-surface" />
      ))}
    </div>
  );
}

function ErrorPanel({ error }: { error: unknown }) {
  return (
    <div className="rounded border border-bad/40 bg-bad/10 p-4 text-sm">
      <p className="text-ink">读取服务状态失败。</p>
      <p className="mt-2 font-mono text-xs text-ink-muted">
        {error instanceof Error ? error.message : String(error)}
      </p>
    </div>
  );
}
