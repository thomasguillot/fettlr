import type * as React from "react";

import { cn, focusRing } from "../lib/utils";

export function Input({ className, type = "text", ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "flex h-9 w-full rounded-md border border-border-strong bg-surface px-2.5 py-1",
        "text-sm text-text transition-colors",
        "placeholder:text-text-muted",
        "disabled:cursor-not-allowed disabled:opacity-45",
        "aria-invalid:border-danger-text",
        focusRing,
        className,
      )}
      {...props}
    />
  );
}

export function Label({ className, ...props }: React.ComponentProps<"label">) {
  return (
    <label
      data-slot="label"
      className={cn("flex items-center gap-1.5 text-sm font-medium text-text select-none", className)}
      {...props}
    />
  );
}
