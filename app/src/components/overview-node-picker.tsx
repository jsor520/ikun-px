import { ArrowRight, Check, ChevronRight, Gauge } from "lucide-react";

import { Flag } from "@/components/flag";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  countryOf,
  latencyColor,
  qualityColor,
} from "@/lib/format";
import type { Node, NodeMeasurement } from "@/lib/api";

interface OverviewNodePickerProps {
  nodes: Node[];
  measurements: Record<string, NodeMeasurement>;
  targetUuid: string;
  currentUuid?: string;
  online: boolean;
  busy: boolean;
  onSelect: (uuid: string) => void;
  onConnect: () => void;
  onViewAll: () => void;
  showIcons: boolean;
}

export function OverviewNodePicker({
  nodes,
  measurements,
  targetUuid,
  currentUuid,
  online,
  busy,
  onSelect,
  onConnect,
  onViewAll,
  showIcons,
}: OverviewNodePickerProps) {
  const current = nodes.find((node) => node.uuid === currentUuid);
  const fastest = [...nodes].sort((left, right) => {
    const a = measurements[left.uuid]?.latency_ms ?? Number.MAX_SAFE_INTEGER;
    const b = measurements[right.uuid]?.latency_ms ?? Number.MAX_SAFE_INTEGER;
    return a - b;
  });
  const recommendations = [
    ...(current ? [current] : []),
    ...fastest.filter((node) => node.uuid !== current?.uuid),
  ].slice(0, 4);
  const target = nodes.find((node) => node.uuid === targetUuid);
  const isCurrent = online && target?.uuid === currentUuid;

  return (
    <section className="my-4 rounded-2xl border bg-card p-4 shadow-sm">
      <div className="mb-3 flex items-center justify-between">
        <div>
          <h2 className="text-[15px] font-semibold text-foreground">
            快速选择节点
          </h2>
          <p className="mt-0.5 text-[11px] text-muted-foreground">
            根据实时延迟推荐，选择后确认连接，避免误切换
          </p>
        </div>
        <Button variant="ghost" size="sm" onClick={onViewAll} className="text-xs">
          查看全部节点
          <ChevronRight className="size-4" />
        </Button>
      </div>

      <div className="grid grid-cols-4 gap-2.5">
        {recommendations.map((node) => {
          const selected = node.uuid === targetUuid;
          const measurement = measurements[node.uuid];
          return (
            <button
              key={node.uuid}
              type="button"
              onClick={() => onSelect(node.uuid)}
              className={cn(
                "relative flex min-w-0 items-center gap-3 rounded-xl border px-3 py-3 text-left transition",
                selected
                  ? "border-primary/55 bg-accent shadow-[inset_0_0_0_1px_var(--accent)]"
                  : "bg-surface-1 hover:border-primary/30 hover:bg-accent/50",
              )}
            >
              {showIcons && (
                <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-surface-2">
                  <Flag
                    country={countryOf(node)}
                    className="h-[17px] w-[25px] rounded-[3px]"
                  />
                </span>
              )}
              <span className="min-w-0 flex-1">
                <strong className="block truncate text-[12px] font-semibold text-card-foreground">
                  {node.name}
                </strong>
                <small className="mt-1 flex items-center gap-1 text-[10px]">
                  <Gauge className="size-3" />
                  {measurement
                    ? (
                      <>
                        <span className={latencyColor(measurement.latency_ms)}>
                          {measurement.latency_ms == null
                            ? "超时"
                            : `${measurement.latency_ms} ms`}
                        </span>
                        <span className="text-muted-foreground">·</span>
                        <span
                          className={qualityColor(
                            measurement.successful,
                            measurement.attempts,
                          )}
                        >
                          {measurement.successful}/{measurement.attempts}
                        </span>
                      </>
                    )
                    : "正在测速…"}
                </small>
              </span>
              {selected && (
                <span className="absolute top-2 right-2 grid size-4 place-items-center rounded-full bg-primary text-primary-foreground">
                  <Check className="size-2.5" />
                </span>
              )}
            </button>
          );
        })}
      </div>

      <div className="mt-3 flex items-center justify-between rounded-xl bg-surface-1 px-4 py-3">
        <div className="flex min-w-0 items-center gap-3">
          {showIcons && (
            <Flag
              country={target ? countryOf(target) : ""}
              className="h-5 w-[30px] rounded"
            />
          )}
          <div className="min-w-0">
            <small className="block text-[10px] text-muted-foreground">
              {isCurrent ? "当前连接" : "准备连接"}
            </small>
            <strong className="block truncate text-[13px] text-card-foreground">
              {target?.name || "请选择节点"}
            </strong>
          </div>
        </div>
        <Button
          disabled={!target || busy || isCurrent}
          onClick={onConnect}
          className="h-9 gap-4 px-4 text-xs"
        >
          {busy ? "正在切换…" : isCurrent ? "当前节点" : online ? "切换并连接" : "连接此节点"}
          <ArrowRight className="size-4" />
        </Button>
      </div>
    </section>
  );
}
