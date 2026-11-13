import { CalendarClock, Gauge, Server } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { formatDate, formatTraffic } from "@/lib/format";
import type { UserInfo } from "@/lib/api";

interface StatCardProps {
  icon: LucideIcon;
  tone: "violet" | "aqua" | "amber";
  label: string;
  value: string;
  caption: string;
  className?: string;
}

const TONE_CLASS = {
  violet: "bg-primary/12 text-neon",
  aqua: "bg-online/12 text-online-foreground",
  amber: "bg-amber/12 text-amber",
} as const;

function StatCard({
  icon: Icon,
  tone,
  label,
  value,
  caption,
  className,
}: StatCardProps) {
  return (
    <Card
      className={cn(
        "grid min-w-0 grid-cols-[40px_1fr_auto] items-center gap-0 rounded-[14px] border px-[15px] py-3.5 ring-0",
        className,
      )}
    >
      <span
        className={cn(
          "grid size-[31px] place-items-center rounded-[9px]",
          TONE_CLASS[tone],
        )}
      >
        <Icon className="size-4" />
      </span>
      <div className="min-w-0">
        <small className="block text-[11px] text-muted-foreground">{label}</small>
        <strong className="mt-0.5 block truncate text-base text-card-foreground">
          {value}
        </strong>
      </div>
      <span className="text-[10px] text-muted-foreground">{caption}</span>
    </Card>
  );
}

interface StatCardsProps {
  user: UserInfo | null;
  nodeCount: number;
  regionCount: number;
}

export function StatCards({ user, nodeCount, regionCount }: StatCardsProps) {
  return (
    <section className="mt-3.5 mb-[38px] grid grid-cols-3 gap-3 max-[980px]:grid-cols-2">
      <StatCard
        icon={Gauge}
        tone="violet"
        label="剩余流量"
        value={user ? formatTraffic(user.transfer) : "—"}
        caption="当前套餐"
      />
      <StatCard
        icon={CalendarClock}
        tone="aqua"
        label="有效期至"
        value={user ? formatDate(user.charge_expire_date) : "—"}
        caption="账户有效"
      />
      <StatCard
        icon={Server}
        tone="amber"
        label="可用节点"
        value={String(nodeCount || "—")}
        caption={`${regionCount || "—"} 个地区`}
        className="max-[980px]:hidden"
      />
    </section>
  );
}
