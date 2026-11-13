import { useEffect, useState, type ComponentProps } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Moon, Square, SquareStack, Sun, X } from "lucide-react";

import { BrandLogo } from "@/components/brand-logo";
import { useTheme } from "@/components/theme-provider";
import { cn } from "@/lib/utils";

const appWindow = getCurrentWindow();

function ControlButton({
  className,
  ...props
}: ComponentProps<"button"> & { title: string }) {
  return (
    <button
      type="button"
      aria-label={props.title}
      className={cn(
        "grid h-full w-[46px] place-items-center text-muted-foreground transition-colors outline-none",
        "hover:bg-surface-2 hover:text-foreground focus-visible:bg-surface-2 focus-visible:text-foreground",
        className,
      )}
      {...props}
    />
  );
}

export function TitleBar() {
  const { theme, toggle } = useTheme();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let active = true;
    const sync = () =>
      void appWindow
        .isMaximized()
        .then((value) => active && setMaximized(value))
        .catch(() => undefined);

    sync();
    const unlisten = appWindow.onResized(sync);
    return () => {
      active = false;
      void unlisten.then((dispose) => dispose());
    };
  }, []);

  return (
    <header
      data-tauri-drag-region
      className="flex h-9 shrink-0 items-center justify-between border-b border-border bg-titlebar pl-3 backdrop-blur-2xl"
    >
      {/* Ignores the pointer so the header behind it stays the drag surface. */}
      <div className="pointer-events-none flex items-center gap-2">
        <BrandLogo className="size-[17px] rounded-[5px]" />
        <span className="text-[11px] font-semibold tracking-[0.09em] text-foreground/80">
          IKUN PX
        </span>
      </div>

      <div className="flex h-full items-stretch">
        <ControlButton
          title={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
          onClick={toggle}
          className="w-10"
        >
          {theme === "dark" ? (
            <Sun className="size-[15px]" />
          ) : (
            <Moon className="size-[15px]" />
          )}
        </ControlButton>
        <ControlButton title="最小化" onClick={() => void appWindow.minimize()}>
          <Minus className="size-[15px]" />
        </ControlButton>
        <ControlButton
          title={maximized ? "向下还原" : "最大化"}
          onClick={() => void appWindow.toggleMaximize()}
        >
          {maximized ? (
            <SquareStack className="size-[13px]" />
          ) : (
            <Square className="size-[12px]" />
          )}
        </ControlButton>
        <ControlButton
          title="关闭到托盘"
          onClick={() => void appWindow.close()}
          className="hover:bg-[#c42b1c] hover:text-white focus-visible:bg-[#c42b1c] focus-visible:text-white"
        >
          <X className="size-4" />
        </ControlButton>
      </div>
    </header>
  );
}
