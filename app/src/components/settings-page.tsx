import {
  CircleGauge,
  Flag,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";

import { cn } from "@/lib/utils";

interface SettingsPageProps {
  showNodeIcons: boolean;
  sidebarCollapsed: boolean;
  saving: boolean;
  onChange: (showNodeIcons: boolean, sidebarCollapsed: boolean) => void;
}

function Toggle({
  checked,
  disabled,
  onChange,
}: {
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative h-7 w-12 rounded-full border transition disabled:opacity-60",
        checked
          ? "border-primary bg-primary"
          : "border-border bg-surface-3",
      )}
    >
      <span
        className={cn(
          "absolute top-1 size-[18px] rounded-full bg-white shadow-sm transition",
          checked ? "left-[25px]" : "left-1",
        )}
      />
    </button>
  );
}

export function SettingsPage({
  showNodeIcons,
  sidebarCollapsed,
  saving,
  onChange,
}: SettingsPageProps) {
  return (
    <section className="space-y-4">
      <div className="overflow-hidden rounded-2xl border bg-card">
        <div className="border-b bg-surface-1 px-5 py-4">
          <h2 className="text-base font-semibold">节点显示</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            设置会立即生效，并在下次启动时保持。
          </p>
        </div>
        <div className="flex items-center justify-between gap-6 px-5 py-5">
          <div className="flex items-center gap-4">
            <span className="grid size-11 place-items-center rounded-xl bg-primary/10 text-primary">
              <Flag className="size-5" />
            </span>
            <div>
              <strong className="text-sm font-semibold">显示节点国家图标</strong>
              <p className="mt-1 text-xs text-muted-foreground">
                默认关闭。开启后节点列表、概览推荐和连接栏显示国旗图标。
              </p>
            </div>
          </div>
          <Toggle
            checked={showNodeIcons}
            disabled={saving}
            onChange={(checked) => onChange(checked, sidebarCollapsed)}
          />
        </div>
      </div>

      <div className="overflow-hidden rounded-2xl border bg-card">
        <div className="flex items-center justify-between gap-6 px-5 py-5">
          <div className="flex items-center gap-4">
            <span className="grid size-11 place-items-center rounded-xl bg-primary/10 text-primary">
              {sidebarCollapsed ? (
                <PanelLeftOpen className="size-5" />
              ) : (
                <PanelLeftClose className="size-5" />
              )}
            </span>
            <div>
              <strong className="text-sm font-semibold">折叠左侧菜单栏</strong>
              <p className="mt-1 text-xs text-muted-foreground">
                折叠后仅保留导航图标，扩大主要内容区域。
              </p>
            </div>
          </div>
          <Toggle
            checked={sidebarCollapsed}
            disabled={saving}
            onChange={(checked) => onChange(showNodeIcons, checked)}
          />
        </div>
      </div>

      <div className="rounded-2xl border bg-card p-5">
        <div className="mb-4 flex items-center gap-3">
          <span className="grid size-10 place-items-center rounded-xl bg-primary/10 text-primary">
            <CircleGauge className="size-5" />
          </span>
          <div>
            <h2 className="text-sm font-semibold">延迟颜色说明</h2>
            <p className="mt-0.5 text-xs text-muted-foreground">
              延迟和成功率分别着色，不影响节点选择。
            </p>
          </div>
        </div>
        <div className="grid grid-cols-5 gap-2 text-center text-xs">
          <span className="rounded-lg bg-online/10 px-3 py-2 text-online-foreground">
            ≤80 ms · 优秀
          </span>
          <span className="rounded-lg bg-amber/10 px-3 py-2 text-amber">
            81–180 ms · 正常
          </span>
          <span className="rounded-lg bg-orange-500/10 px-3 py-2 text-orange-500 dark:text-orange-300">
            181–350 ms · 偏慢
          </span>
          <span className="rounded-lg bg-destructive/10 px-3 py-2 text-destructive">
            &gt;350 ms · 较差
          </span>
          <span className="rounded-lg bg-surface-2 px-3 py-2 text-muted-foreground">
            超时 · 不可用
          </span>
        </div>
      </div>
    </section>
  );
}
