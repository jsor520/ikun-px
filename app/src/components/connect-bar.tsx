import { ArrowRight } from "lucide-react";

import { Flag } from "@/components/flag";
import { Button } from "@/components/ui/button";
import { countryOf } from "@/lib/format";
import type { Node } from "@/lib/api";

interface ConnectBarProps {
  targetNode: Node | undefined;
  disabled: boolean;
  label: string;
  onConnect: () => void;
  showIcon: boolean;
}

export function ConnectBar({
  targetNode,
  disabled,
  label,
  onConnect,
  showIcon,
}: ConnectBarProps) {
  return (
    <div className="sticky bottom-3 z-4 mt-4 flex items-center justify-between gap-5 rounded-2xl border border-primary/20 bg-titlebar px-4 py-3.5 shadow-[0_16px_45px_rgba(24,20,52,0.12)] backdrop-blur-[18px] dark:shadow-[0_16px_45px_rgba(0,0,0,0.32)]">
      <div className="flex min-w-0 items-center gap-3">
        {showIcon && (
          <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-surface-2">
            <Flag
              country={targetNode ? countryOf(targetNode) : ""}
              className="h-[17px] w-[25px] rounded-[3px] shadow-[0_1px_3px_rgba(0,0,0,0.45)]"
            />
          </span>
        )}
        <div className="min-w-0">
          <small className="block text-[10px] text-muted-foreground">准备连接</small>
          <strong className="mt-0.5 block max-w-[420px] truncate text-[13px] text-card-foreground">
            {targetNode?.name || "请选择节点"}
          </strong>
        </div>
      </div>

      <Button
        disabled={disabled}
        onClick={onConnect}
        className="h-10 gap-5 rounded-xl bg-[linear-gradient(135deg,#7963f3,#5a42cd)] px-5 text-xs font-bold text-white shadow-[0_7px_20px_rgba(91,65,210,0.28)] transition-[150ms] duration-150 hover:-translate-y-px hover:brightness-112 disabled:bg-surface-3 disabled:bg-none disabled:text-muted-foreground disabled:shadow-none"
      >
        {label}
        <ArrowRight className="size-3.5" />
      </Button>
    </div>
  );
}
