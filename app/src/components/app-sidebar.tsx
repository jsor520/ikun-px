import { LogIn, LogOut } from "lucide-react";

import { BrandLogo } from "@/components/brand-logo";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import { SECTIONS, type SectionId } from "@/lib/sections";
import type { EngineStatus, MarsConfig } from "@/lib/api";

interface AppSidebarProps {
  config: MarsConfig | null;
  status: EngineStatus | null;
  nodeCount: number;
  online: boolean;
  active: SectionId;
  onNavigate: (section: SectionId) => void;
  onLogout: () => void;
  onLogin: () => void;
  collapsed: boolean;
}

export function AppSidebar({
  config,
  status,
  nodeCount,
  online,
  active,
  onNavigate,
  onLogout,
  onLogin,
  collapsed,
}: AppSidebarProps) {
  const loggedIn = config?.logged_in ?? false;

  return (
    <aside
      className={cn(
        "z-5 flex h-full flex-col border-r border-sidebar-border bg-sidebar pt-[26px] pb-5 backdrop-blur-[20px] transition-[padding] duration-200",
        collapsed ? "px-3" : "px-[18px]",
      )}
    >
      <div
        className={cn(
          "flex items-center gap-3 pb-7",
          collapsed ? "justify-center px-0" : "px-2",
        )}
      >
        <BrandLogo className="size-[38px] rounded-[11px] shadow-[0_7px_28px_rgba(103,78,255,0.35)]" />
        <div className={cn(collapsed && "hidden")}>
          <strong className="block text-base tracking-[0.08em] text-foreground">
            IKUN PX
          </strong>
          <small className="mt-px block text-[11px] tracking-[0.08em] text-muted-foreground uppercase">
            Secure network
          </small>
        </div>
      </div>

      <ScrollArea
        className="-mx-1 min-h-0 flex-1 px-1"
        contentClassName="flex min-h-full flex-col"
      >
        <nav aria-label="主导航" className="flex flex-col gap-1.5">
          {SECTIONS.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => onNavigate(item.id)}
              title={collapsed ? item.label : undefined}
              aria-current={active === item.id ? "page" : undefined}
              className={cn(
                "grid min-h-11 cursor-pointer items-center rounded-xl border border-transparent text-left text-sm font-semibold transition-[160ms] duration-160 outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                collapsed
                  ? "grid-cols-1 place-items-center px-0"
                  : "grid-cols-[26px_1fr_auto] px-3",
                active === item.id
                  ? "border-sidebar-accent bg-sidebar-accent text-sidebar-accent-foreground"
                  : "text-sidebar-foreground hover:bg-surface-2 hover:text-foreground",
              )}
            >
              <item.icon className="size-[18px] text-sidebar-primary" />
              <span className={cn(collapsed && "hidden")}>{item.label}</span>
              {item.id === "nodes" && !collapsed && (
                <Badge
                  variant="secondary"
                  className="min-w-[22px] rounded-[7px] bg-surface-2 px-1.5 text-[10px] font-normal text-muted-foreground"
                >
                  {nodeCount || "—"}
                </Badge>
              )}
            </button>
          ))}
        </nav>

        <div className="flex-1" />

        {collapsed ? (
          <div
            title={online ? "服务运行中" : "服务未连接"}
            className={cn(
              "mt-6 grid h-11 place-items-center rounded-xl border",
              online
                ? "border-online/25 bg-online/5"
                : "border-border bg-surface-1",
            )}
          >
            <span
              className={cn(
                "size-2 rounded-full",
                online
                  ? "bg-online shadow-[0_0_10px_var(--online)]"
                  : "bg-muted-foreground",
              )}
            />
          </div>
        ) : (
          <div
            className={cn(
              "mt-6 rounded-[13px] border p-[13px]",
              online
                ? "border-online/25 bg-online/5"
                : "border-border bg-surface-1",
            )}
          >
            <div
              className={cn(
                "flex items-center gap-2 text-xs font-semibold",
                online ? "text-online-foreground" : "text-muted-foreground",
              )}
            >
              <span
                className={cn(
                  "size-[7px] rounded-full",
                  online
                    ? "bg-online shadow-[0_0_10px_var(--online)]"
                    : "bg-muted-foreground",
                )}
              />
              {online ? "服务运行中" : "服务未连接"}
            </div>

            <dl className="mt-3 grid grid-cols-2 gap-[7px]">
              <div className="rounded-lg bg-surface-1 p-2">
                <dt className="text-[9px] font-bold tracking-[0.08em] text-muted-foreground">
                  HTTP
                </dt>
                <dd className="mt-0.5 font-mono text-[11px] text-card-foreground">
                  {status?.http_proxy_open
                    ? status.http_address.replace("127.0.0.1", "")
                    : "—"}
                </dd>
              </div>
              <div className="rounded-lg bg-surface-1 p-2">
                <dt className="text-[9px] font-bold tracking-[0.08em] text-muted-foreground">
                  SOCKS5
                </dt>
                <dd className="mt-0.5 font-mono text-[11px] text-card-foreground">
                  {status?.socks5_ok
                    ? status.socks5_address.replace("127.0.0.1", "")
                    : "—"}
                </dd>
              </div>
            </dl>
          </div>
        )}
      </ScrollArea>

      <div
        className={cn(
          "mt-3 items-center border-t border-sidebar-border pt-3",
          collapsed
            ? "flex flex-col gap-2 px-0"
            : "grid grid-cols-[34px_minmax(0,1fr)_auto] gap-[9px] px-[7px]",
        )}
      >
        <Avatar className="size-8 rounded-[10px]">
          <AvatarFallback
            className={cn(
              "rounded-[10px] text-xs font-extrabold text-white",
              loggedIn
                ? "bg-[linear-gradient(145deg,#8068ff,#4e3ac4)]"
                : "bg-surface-2 text-muted-foreground",
            )}
          >
            {(config?.email || "U").slice(0, 1).toUpperCase()}
          </AvatarFallback>
        </Avatar>
        <div className={cn("min-w-0", collapsed && "hidden")}>
          <strong className="block truncate text-[11px] text-card-foreground">
            {config?.email || "未登录"}
          </strong>
          <small className="mt-0.5 block truncate text-[10px] text-muted-foreground">
            {loggedIn ? "标准账户" : "访客浏览"}
          </small>
        </div>
        {loggedIn ? (
          <button
            type="button"
            onClick={onLogout}
            title="退出登录"
            aria-label="退出登录"
            className="rounded-md p-1 text-muted-foreground transition hover:bg-surface-2 hover:text-destructive"
          >
            <LogOut className="size-3.5" />
          </button>
        ) : (
          <button
            type="button"
            onClick={onLogin}
            title="登录"
            aria-label="登录"
            className="rounded-md p-1 text-muted-foreground transition hover:bg-surface-2 hover:text-primary"
          >
            <LogIn className="size-3.5" />
          </button>
        )}
      </div>
    </aside>
  );
}
