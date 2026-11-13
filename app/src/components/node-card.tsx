import { Flag } from "@/components/flag";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import {
  asNumber,
  countryOf,
  latencyColor,
  qualityColor,
} from "@/lib/format";
import type { Node, NodeMeasurement } from "@/lib/api";

interface NodeCardProps {
  node: Node;
  selected: boolean;
  active: boolean;
  onSelect: (uuid: string) => void;
  measurement?: NodeMeasurement;
  showIcon: boolean;
}

export function NodeCard({
  node,
  selected,
  active,
  onSelect,
  measurement,
  showIcon,
}: NodeCardProps) {
  const bandwidth = asNumber(node.bandwidth_current);
  const total = asNumber(node.bandwidth_total);
  const load = total > 0 ? Math.min(100, (bandwidth / total) * 100) : 0;

  return (
    <button
      type="button"
      onClick={() => onSelect(node.uuid)}
      className={cn(
        "relative grid min-h-[86px] min-w-0 grid-rows-[auto_4px] items-center gap-3 rounded-2xl border px-4 pt-4 pb-3 text-left transition-[150ms] duration-150 outline-none focus-visible:ring-3 focus-visible:ring-ring/50",
        showIcon
          ? "grid-cols-[48px_minmax(0,1fr)_auto_20px]"
          : "grid-cols-[minmax(0,1fr)_auto_20px]",
        "hover:-translate-y-px hover:border-primary/30 hover:bg-accent/60",
        selected
          ? "border-primary/50 bg-accent shadow-[inset_0_0_0_1px_var(--accent)]"
          : active
            ? "border-online/35 bg-surface-1"
            : "bg-surface-1",
      )}
    >
      {showIcon && (
        <span className="grid size-11 place-items-center rounded-xl bg-surface-2">
          <Flag
            country={countryOf(node)}
            className="h-5 w-[30px] rounded-[4px] shadow-[0_1px_3px_rgba(0,0,0,0.45)]"
          />
        </span>
      )}

      <span className="min-w-0">
        <strong className="block truncate text-sm font-semibold text-card-foreground">
          {node.name}
        </strong>
        <small className="mt-1.5 block truncate font-mono text-[11px] text-muted-foreground">
          {node.addr}:{node.port}
        </small>
      </span>

      <span className="min-w-[92px] text-right">
        {active ? (
          <Badge className="h-auto rounded-md bg-online/10 px-1.5 py-0.5 text-[10px] font-bold text-online-foreground">
            已连接
          </Badge>
        ) : node.udp ? (
          <Badge className="h-auto rounded-md bg-primary/10 px-1.5 py-0.5 text-[10px] font-bold text-accent-foreground">
            AEAD
          </Badge>
        ) : null}
        {measurement ? (
          <small className="mt-1.5 block text-[11px]">
            <span className={latencyColor(measurement.latency_ms)}>
              {measurement.latency_ms == null
                ? "超时"
                : `${measurement.latency_ms} ms`}
            </span>
            <span className="text-muted-foreground"> · </span>
            <span
              className={qualityColor(
                measurement.successful,
                measurement.attempts,
              )}
            >
              {measurement.successful}/{measurement.attempts}
            </span>
          </small>
        ) : (
          <small className="mt-1.5 block text-[11px] text-muted-foreground">
            {bandwidth ? `${bandwidth} Mbps` : "待测速"}
          </small>
        )}
      </span>

      <span
        className={cn(
          "grid size-[18px] place-items-center rounded-full border",
          selected ? "border-neon" : "border-muted-foreground/45",
        )}
      >
        {selected && (
          <i className="size-2 rounded-full bg-neon shadow-[0_0_8px_var(--neon)]" />
        )}
      </span>

      <Progress
        value={load}
        className={cn(
          "gap-0 [&_[data-slot=progress-indicator]]:bg-[linear-gradient(90deg,var(--primary),var(--neon-soft))] [&_[data-slot=progress-track]]:h-[3px] [&_[data-slot=progress-track]]:bg-surface-2",
          showIcon ? "col-start-2 col-end-4" : "col-start-1 col-end-3",
        )}
      />
    </button>
  );
}
