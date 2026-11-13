import { useEffect, useMemo, useState } from "react";
import {
  ClipboardCopy,
  Eraser,
  Network,
  RefreshCw,
  SquareTerminal,
  Stethoscope,
  Wrench,
} from "lucide-react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  clearDomainEvents,
  clearDomainRoute,
  copyProxyAddresses,
  copyProxyEnvironment,
  type DiagnosticProbe,
  type DiagnosticReport,
  type DomainEvent,
  type EngineStatus,
  getDomainEvents,
  getPinnedDomains,
  type MarsConfig,
  networkDiagnostics,
  openProxyConsole,
  repairNetwork,
  setDomainRoute,
  setIntegrations,
} from "@/lib/api";
import {
  errorText,
  formatSpeed,
  formatTraffic,
  latencyColor,
} from "@/lib/format";
import { cn } from "@/lib/utils";

interface ProxyManagerProps {
  config: MarsConfig | null;
  status: EngineStatus | null;
  onConfigChanged: () => void;
}

export function ProxyManager({
  config,
  status,
  onConfigChanged,
}: ProxyManagerProps) {
  const [events, setEvents] = useState<DomainEvent[]>([]);
  const [pinned, setPinned] = useState<Map<string, "direct" | "proxy">>(
    new Map(),
  );
  const [filter, setFilter] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [busy, setBusy] = useState(false);
  const [routingTarget, setRoutingTarget] = useState("");
  const [diagnostics, setDiagnostics] = useState<DiagnosticReport | null>(null);
  const [diagnosing, setDiagnosing] = useState(false);

  async function refresh() {
    try {
      setEvents(await getDomainEvents());
    } catch {
      setEvents([]);
    }
  }

  /** The probes dial the node and pull a sample page, so this runs for a few
   *  seconds and the button has to say so. */
  async function runDiagnostics() {
    if (diagnosing) return;
    setDiagnosing(true);
    try {
      setDiagnostics(await networkDiagnostics());
    } catch (cause) {
      toast.error(errorText(cause));
    } finally {
      setDiagnosing(false);
    }
  }

  async function refreshPinned() {
    try {
      const rules = await getPinnedDomains();
      setPinned(
        new Map([
          ...rules.direct.map((host) => [host, "direct"] as const),
          ...rules.proxy.map((host) => [host, "proxy"] as const),
        ]),
      );
    } catch {
      setPinned(new Map());
    }
  }

  useEffect(() => {
    void refreshPinned();
  }, []);

  useEffect(() => {
    void refresh();
    if (!autoRefresh) return;
    const timer = window.setInterval(() => void refresh(), 1500);
    return () => window.clearInterval(timer);
  }, [autoRefresh]);

  const visible = useMemo(() => {
    const needle = filter.trim().toLocaleLowerCase();
    return needle
      ? events.filter((event) => event.host.toLocaleLowerCase().includes(needle))
      : events;
  }, [events, filter]);

  async function updateIntegrations(
    patch: Partial<
      Pick<
        MarsConfig,
        | "git_proxy"
        | "ssh_proxy"
        | "auto_connect"
        | "auto_start"
        | "hotkeys_enabled"
      >
    >,
  ) {
    if (!config || busy) return;
    const next = { ...config, ...patch };
    setBusy(true);
    try {
      await setIntegrations(
        next.git_proxy,
        next.ssh_proxy,
        next.auto_connect,
        next.auto_start,
        next.hotkeys_enabled,
      );
      onConfigChanged();
      toast.success("集成设置已更新");
    } catch (cause) {
      toast.error(errorText(cause));
    } finally {
      setBusy(false);
    }
  }

  /** Clicking the route a domain is already pinned to unpins it, since the
   *  rules file is otherwise only reachable from the rule editor. */
  async function updateDomainRoute(
    host: string,
    route: "direct" | "proxy",
  ) {
    if (routingTarget) return;
    const unpin = pinned.get(host) === route;
    setRoutingTarget(`${route}:${host}`);
    try {
      if (unpin) {
        await clearDomainRoute(host);
        toast.success(`${host} 已恢复为跟随规则`);
      } else {
        await setDomainRoute(host, route);
        toast.success(
          `${host} 之后将${route === "proxy" ? "强制走代理" : "强制走本地"}`,
        );
      }
      await refreshPinned();
    } catch (cause) {
      toast.error(errorText(cause));
    } finally {
      setRoutingTarget("");
    }
  }

  return (
    <section id="proxy-manager">
      <div className="mb-4 flex items-center justify-end">
        <div className="flex flex-wrap justify-end gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              void copyProxyAddresses().then(() =>
                toast.success("本地代理地址已复制"),
              )
            }
          >
            <Network className="size-3.5" />
            代理地址
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={diagnosing}
            title="测量当前连接地址的握手延迟，并通过本地代理实测网页打开速度"
            onClick={() => void runDiagnostics()}
          >
            <Stethoscope className={cn("size-3.5", diagnosing && "animate-pulse")} />
            {diagnosing ? "诊断中…" : "网络诊断"}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              void copyProxyEnvironment().then(() =>
                toast.success("代理环境变量已复制"),
              )
            }
          >
            <ClipboardCopy className="size-3.5" />
            环境变量
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void openProxyConsole()}
          >
            <SquareTerminal className="size-3.5" />
            代理控制台
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              void repairNetwork()
                .then((message) => toast.success(message))
                .catch((cause) => toast.error(errorText(cause)))
            }
          >
            <Wrench className="size-3.5" />
            修复网络
          </Button>
        </div>
      </div>

      {diagnostics && (
        <>
          <div className="mb-2 grid grid-cols-2 gap-2">
            <ProbeCard
              title="当前连接地址"
              probe={diagnostics.node}
              fallback="还没有可用节点，登录并选择节点后再诊断"
            />
            <ProbeCard
              title="模拟网页访问"
              probe={diagnostics.web}
              fallback="连接后这里会给出网页打开耗时和下载速度"
            />
          </div>
          <div className="mb-3 grid grid-cols-2 gap-2 rounded-xl border bg-surface-1 p-3">
            {diagnostics.checks.map((check) => (
              <div
                key={check.name}
                className="flex items-center justify-between rounded-lg bg-surface-1 px-3 py-2 text-[10px]"
              >
                <span
                  className={check.ok ? "text-online-foreground" : "text-amber"}
                >
                  {check.ok ? "正常" : "注意"} · {check.name}
                </span>
                <span className="ml-3 truncate text-muted-foreground">
                  {check.detail}
                </span>
              </div>
            ))}
          </div>
        </>
      )}

      <div className="mb-3 grid grid-cols-5 gap-2 rounded-xl border bg-surface-1 p-3">
        {[
          ["git_proxy", "Git 代理"],
          ["ssh_proxy", "SSH 代理"],
          ["auto_connect", "运行自动加速"],
          ["auto_start", "开机自启"],
          ["hotkeys_enabled", "应用热键"],
        ].map(([key, label]) => (
          <label
            key={key}
            className="flex cursor-pointer items-center gap-2 rounded-lg px-2 py-2 text-[10px] text-card-foreground hover:bg-surface-1"
          >
            <input
              type="checkbox"
              disabled={!config || busy}
              checked={Boolean(config?.[key as keyof MarsConfig])}
              onChange={(event) =>
                void updateIntegrations({
                  [key]: event.target.checked,
                } as Partial<MarsConfig>)
              }
              className="accent-primary"
            />
            {label}
          </label>
        ))}
      </div>

      <div className="overflow-hidden rounded-xl border bg-surface-1">
        <div className="flex items-center gap-2 border-b border-hairline p-2.5">
          <Network className="ml-1 size-4 text-primary" />
          <Input
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="过滤访问域名（支持普通关键词）"
            className="h-8 flex-1 bg-surface-1 text-[10px] dark:bg-surface-1"
          />
          <label className="flex items-center gap-1.5 px-2 text-[9px] text-muted-foreground">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(event) => setAutoRefresh(event.target.checked)}
              className="accent-primary"
            />
            自动刷新
          </label>
          <Button variant="ghost" size="icon-sm" onClick={() => void refresh()}>
            <RefreshCw className="size-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() =>
              void clearDomainEvents().then(() => {
                setEvents([]);
                toast.success("访问记录已清空");
              })
            }
          >
            <Eraser className="size-3.5" />
          </Button>
        </div>

        <div className="grid grid-cols-[minmax(0,1fr)_92px_96px_92px_160px] border-b border-hairline bg-surface-1 px-4 py-2.5 text-[11px] font-semibold text-muted-foreground">
          <span>访问域名</span>
          <span title="这次连接当时实际走的线路，是历史记录">本次走向</span>
          <span title="之后再访问该域名会走哪里">当前规则</span>
          <span>访问时间</span>
          <span className="text-right">操作</span>
        </div>
        <ScrollArea className="max-h-64" viewportClassName="max-h-64">
          {visible.length ? (
            visible.map((event, index) => {
              const pin = pinned.get(event.host);
              return (
                <div
                  key={`${event.timestamp}-${event.host}-${index}`}
                  className="grid grid-cols-[minmax(0,1fr)_92px_96px_92px_160px] items-center border-b border-hairline px-4 py-2.5 text-[11px]"
                >
                  <span className="truncate font-mono text-card-foreground">
                    {event.host}
                  </span>
                  <Badge
                    className={
                      event.route === "proxy"
                        ? "w-fit bg-primary/10 text-accent-foreground"
                        : "w-fit bg-online/10 text-online-foreground"
                    }
                  >
                    {event.route === "proxy" ? "走代理" : "走本地"}
                  </Badge>
                  {pin ? (
                    <Badge
                      className={
                        pin === "proxy"
                          ? "w-fit bg-primary/10 text-accent-foreground"
                          : "w-fit bg-online/10 text-online-foreground"
                      }
                    >
                      {pin === "proxy" ? "强制代理" : "强制本地"}
                    </Badge>
                  ) : (
                    <span className="text-muted-foreground/70">跟随规则</span>
                  )}
                  <span className="text-muted-foreground">
                    {new Date(event.timestamp * 1000).toLocaleTimeString()}
                  </span>
                  <span className="flex justify-end gap-1.5">
                    <Button
                      type="button"
                      variant={pin === "direct" ? "secondary" : "ghost"}
                      size="xs"
                      disabled={Boolean(routingTarget)}
                      title={
                        pin === "direct"
                          ? "再次点击取消强制，恢复跟随规则"
                          : "之后强制走本地"
                      }
                      onClick={() =>
                        void updateDomainRoute(event.host, "direct")
                      }
                      className="text-[10px]"
                    >
                      {routingTarget === `direct:${event.host}`
                        ? "保存中…"
                        : "走本地"}
                    </Button>
                    <Button
                      type="button"
                      variant={pin === "proxy" ? "secondary" : "ghost"}
                      size="xs"
                      disabled={Boolean(routingTarget)}
                      title={
                        pin === "proxy"
                          ? "再次点击取消强制，恢复跟随规则"
                          : "之后强制走代理"
                      }
                      onClick={() =>
                        void updateDomainRoute(event.host, "proxy")
                      }
                      className="text-[10px]"
                    >
                      {routingTarget === `proxy:${event.host}`
                        ? "保存中…"
                        : "走代理"}
                    </Button>
                  </span>
                </div>
              );
            })
          ) : (
            <div className="grid h-32 place-items-center text-[10px] text-muted-foreground">
              连接后将在这里显示域名分流记录
            </div>
          )}
        </ScrollArea>
      </div>

      <div className="mt-2 text-right text-[9px] text-muted-foreground">
        会话流量：上传 {formatTraffic(String(status?.upload_bytes ?? 0))} · 下载{" "}
        {formatTraffic(String(status?.download_bytes ?? 0))}
      </div>
    </section>
  );
}

function ProbeCard({
  title,
  probe,
  fallback,
}: {
  title: string;
  probe: DiagnosticProbe | null;
  fallback: string;
}) {
  return (
    <div className="rounded-xl border bg-surface-1 p-3">
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-[10px] font-semibold text-muted-foreground">
          {title}
        </span>
        <span className="truncate font-mono text-[10px] text-muted-foreground">
          {probe?.target ?? "—"}
        </span>
      </div>
      {probe ? (
        <>
          <div className="mt-1.5 flex items-baseline gap-3">
            <strong
              className={cn(
                "font-mono text-[17px] tabular-nums",
                probe.ok ? latencyColor(probe.latency_ms) : "text-destructive",
              )}
            >
              {probe.latency_ms == null ? "无响应" : `${probe.latency_ms} ms`}
            </strong>
            {probe.speed_kbps != null && (
              <span className="font-mono text-[12px] tabular-nums text-card-foreground">
                {formatSpeed(probe.speed_kbps)}
              </span>
            )}
          </div>
          <p
            title={probe.detail}
            className={cn(
              "mt-1 truncate text-[10px]",
              probe.ok ? "text-muted-foreground" : "text-amber",
            )}
          >
            {probe.detail}
          </p>
        </>
      ) : (
        <p className="mt-2 text-[10px] text-muted-foreground">{fallback}</p>
      )}
    </div>
  );
}
