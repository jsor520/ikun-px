import { ScrollArea as ScrollAreaPrimitive } from "@base-ui/react/scroll-area"

import { cn } from "@/lib/utils"

type ScrollAreaProps = ScrollAreaPrimitive.Root.Props & {
  orientation?: "vertical" | "horizontal" | "both"
  viewportClassName?: string
  contentClassName?: string
}

function ScrollArea({
  className,
  children,
  orientation = "vertical",
  viewportClassName,
  contentClassName,
  ...props
}: ScrollAreaProps) {
  const showVertical = orientation !== "horizontal"
  const showHorizontal = orientation !== "vertical"

  return (
    <ScrollAreaPrimitive.Root
      data-slot="scroll-area"
      className={cn("relative", className)}
      {...props}
    >
      <ScrollAreaPrimitive.Viewport
        data-slot="scroll-area-viewport"
        className={cn(
          "size-full rounded-[inherit] transition-[color,box-shadow] outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-1",
          viewportClassName
        )}
      >
        {/* Content carries the resize observer that keeps the scrollbars in
            sync, so it must wrap the children on every axis. */}
        <ScrollAreaPrimitive.Content
          data-slot="scroll-area-content"
          className={contentClassName}
          style={showHorizontal ? undefined : { minWidth: 0 }}
        >
          {children}
        </ScrollAreaPrimitive.Content>
      </ScrollAreaPrimitive.Viewport>
      {showVertical && <ScrollBar orientation="vertical" />}
      {showHorizontal && <ScrollBar orientation="horizontal" />}
      <ScrollAreaPrimitive.Corner />
    </ScrollAreaPrimitive.Root>
  )
}

function ScrollBar({
  className,
  orientation = "vertical",
  ...props
}: ScrollAreaPrimitive.Scrollbar.Props) {
  return (
    <ScrollAreaPrimitive.Scrollbar
      data-slot="scroll-area-scrollbar"
      data-orientation={orientation}
      orientation={orientation}
      className={cn(
        "flex touch-none rounded-full bg-surface-1 p-px transition-colors duration-150 select-none hover:bg-surface-2",
        "data-horizontal:h-2.5 data-horizontal:flex-col data-vertical:w-2.5",
        className
      )}
      {...props}
    >
      <ScrollAreaPrimitive.Thumb
        data-slot="scroll-area-thumb"
        className="relative flex-1 rounded-full bg-neon/45 transition-colors hover:bg-neon/75"
      />
    </ScrollAreaPrimitive.Scrollbar>
  )
}

export { ScrollArea, ScrollBar }
