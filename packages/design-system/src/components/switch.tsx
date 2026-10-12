import { Switch as SwitchPrimitive } from "radix-ui";
import type * as React from "react";

import { cn, focusRing } from "../lib/utils";

export function Switch({ className, ...props }: React.ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      className={cn(
        "peer relative inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-border-strong p-px transition-colors",
        "before:absolute before:top-1/2 before:left-0 before:h-6 before:w-full before:-translate-y-1/2 before:content-['']",
        "bg-surface data-[state=checked]:border-blue data-[state=checked]:bg-blue",
        "disabled:cursor-not-allowed disabled:opacity-45",
        focusRing,
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        className={cn(
          "pointer-events-none block size-4 rounded-full bg-text data-[state=checked]:bg-blue-contrast",
          "transition-transform data-[state=checked]:translate-x-4 data-[state=unchecked]:translate-x-0",
        )}
      />
    </SwitchPrimitive.Root>
  );
}
