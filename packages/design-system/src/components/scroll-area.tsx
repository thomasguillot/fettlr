import { ScrollArea as ScrollAreaPrimitive } from "radix-ui";
import type * as React from "react";

import { cn, focusRing } from "../lib/utils";

export function ScrollArea({
  className,
  fillViewport = false,
  children,
  ...props
}: React.ComponentProps<typeof ScrollAreaPrimitive.Root> & {
  /**
   * The Radix-inserted wrapper around the viewport's children is `display: table; height: auto`
   * by default, which defeats any `min-h-full`/`mt-auto` a consumer puts on its own content: a
   * percentage height only resolves against a containing block with an explicitly specified
   * height, and `height: auto` never counts as one, no matter how tall the box ends up. Opt in
   * here to make that wrapper a column flexbox stretched to at least the viewport's own (real,
   * flex-resolved) height, so a direct child can grow into the leftover space with `flex-1` and
   * pin something to its bottom with `mt-auto`; content taller than the viewport still grows the
   * wrapper past that minimum and scrolls normally.
   */
  fillViewport?: boolean;
}) {
  return (
    <ScrollAreaPrimitive.Root
      data-slot="scroll-area"
      className={cn("relative overflow-hidden", className)}
      {...props}
    >
      <ScrollAreaPrimitive.Viewport
        className={cn(
          "size-full rounded-[inherit]",
          fillViewport ? "[&>div]:flex! [&>div]:flex-col [&>div]:min-h-full" : "[&>div]:block!",
          focusRing,
        )}
      >
        {children}
      </ScrollAreaPrimitive.Viewport>
      <ScrollBar />
      <ScrollBar orientation="horizontal" />
      <ScrollAreaPrimitive.Corner />
    </ScrollAreaPrimitive.Root>
  );
}

export function ScrollBar({
  className,
  orientation = "vertical",
  ...props
}: React.ComponentProps<typeof ScrollAreaPrimitive.ScrollAreaScrollbar>) {
  return (
    <ScrollAreaPrimitive.ScrollAreaScrollbar
      orientation={orientation}
      className={cn(
        // Above the sticky rows and headings inside the viewport, which carry their own z-index.
        "z-10 flex touch-none select-none p-0.5 transition-colors",
        orientation === "vertical" ? "h-full w-2.5" : "h-2.5 flex-col",
        className,
      )}
      {...props}
    >
      <ScrollAreaPrimitive.ScrollAreaThumb className="relative flex-1 rounded-full bg-border-strong" />
    </ScrollAreaPrimitive.ScrollAreaScrollbar>
  );
}
