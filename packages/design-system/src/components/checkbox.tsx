import { RiCheckLine } from "@remixicon/react";
import { Checkbox as CheckboxPrimitive } from "radix-ui";
import type * as React from "react";

import { cn, focusRing } from "../lib/utils";

export function Checkbox({
  className,
  ...props
}: React.ComponentProps<typeof CheckboxPrimitive.Root>) {
  return (
    <CheckboxPrimitive.Root
      data-slot="checkbox"
      className={cn(
        "peer size-4 shrink-0 rounded-xs border border-border-strong bg-surface transition-colors",
        "disabled:cursor-not-allowed disabled:opacity-45",
        "data-[state=checked]:border-blue data-[state=checked]:bg-blue data-[state=checked]:text-blue-contrast",
        focusRing,
        className,
      )}
      {...props}
    >
      <CheckboxPrimitive.Indicator className="flex items-center justify-center text-current">
        <RiCheckLine className="size-3" />
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  );
}
