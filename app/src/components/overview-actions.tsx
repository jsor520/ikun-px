import { BadgeDollarSign, CircleHelp, House, RefreshCw } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "sonner";

import { cn } from "@/lib/utils";
import type { MarsConfig, UserInfo } from "@/lib/api";

interface OverviewActionsProps {
  config: MarsConfig | null;
  user: UserInfo | null;
  refreshing: boolean;
  onRefresh: () => void;
  onHelp: () => void;
}

export function OverviewActions({
  config,
  user,
  refreshing,
  onRefresh,
  onHelp,
}: OverviewActionsProps) {
  const webBase = config?.web || "https://web.marslinkvpn.com";

  async function open(path = "") {
    try {
      const base = webBase.endsWith("/") ? webBase : `${webBase}/`;
      await openUrl(new URL(path, base).toString());
    } catch (error) {
      toast.error(`无法打开链接：${String(error)}`);
    }
  }

  const actions = [
    {
      label: "刷新",
      description: "账户与节点",
      icon: RefreshCw,
      action: onRefresh,
      spinning: refreshing,
    },
    {
      label: "充值",
      description: "套餐与流量",
      icon: BadgeDollarSign,
      action: () => void open(`charge?id=${encodeURIComponent(user?.uuid || "")}`),
    },
    {
      label: "官网",
      description: "IKUN PX 代理服务",
      icon: House,
      action: () => void open(),
    },
    {
      label: "帮助",
      description: "诊断与配置",
      icon: CircleHelp,
      action: onHelp,
    },
  ];

  return (
    <section className="mb-4 grid grid-cols-4 gap-3" aria-label="常用操作">
      {actions.map((action) => (
        <button
          key={action.label}
          type="button"
          onClick={action.action}
          className="group flex min-h-[70px] items-center gap-3 rounded-2xl border bg-card px-4 text-left shadow-sm transition hover:-translate-y-0.5 hover:border-primary/35 hover:bg-accent/50 hover:shadow-md"
        >
          <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-primary/10 text-primary transition group-hover:bg-primary group-hover:text-primary-foreground">
            <action.icon
              className={cn("size-5", action.spinning && "animate-spin")}
            />
          </span>
          <span>
            <strong className="block text-[14px] font-semibold text-foreground">
              {action.label}
            </strong>
            <small className="mt-0.5 block text-[11px] text-muted-foreground">
              {action.description}
            </small>
          </span>
        </button>
      ))}
    </section>
  );
}
