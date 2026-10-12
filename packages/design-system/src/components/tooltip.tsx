import { Tooltip as TooltipPrimitive } from "radix-ui";
import type * as React from "react";

import { cn } from "../lib/utils";

export function TooltipProvider({
  delayDuration = 250,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Provider>) {
  return <TooltipPrimitive.Provider delayDuration={delayDuration} {...props} />;
}

export const Tooltip = TooltipPrimitive.Root;
export const TooltipTrigger = TooltipPrimitive.Trigger;

export function TooltipContent({
  className,
  sideOffset = 6,
  children,
  ...props
}: React.ComponentProps<typeof TooltipPrimitive.Content>) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        sideOffset={sideOffset}
        className={cn(
          "z-50 max-w-64 rounded-md bg-inverse px-2.5 py-1.5 text-sm text-inverse-contrast",
          "origin-[var(--radix-tooltip-content-transform-origin)]",
          "data-[state=delayed-open]:animate-pop-in data-[state=closed]:animate-pop-out",
          className,
        )}
        {...props}
      >
        {children}
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  );
}
