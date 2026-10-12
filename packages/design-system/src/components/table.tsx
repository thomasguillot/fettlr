import type * as React from "react";

import { cn } from "../lib/utils";

export function Table({
  className,
  containerClassName,
  ...props
}: React.ComponentProps<"table"> & { containerClassName?: string }) {
  return (
    <div className={cn("relative w-full overflow-x-auto", containerClassName)}>
      <table
        data-slot="table"
        className={cn("w-full caption-bottom border-separate border-spacing-0 text-sm", className)}
        {...props}
      />
    </div>
  );
}

export function TableHeader({ className, ...props }: React.ComponentProps<"thead">) {
  return <thead className={cn("bg-bg-subtle [&_th]:border-b [&_th]:border-border", className)} {...props} />;
}

export function TableBody({ className, ...props }: React.ComponentProps<"tbody">) {
  return <tbody className={className} {...props} />;
}

export function TableRow({ className, ...props }: React.ComponentProps<"tr">) {
  return (
    <tr
      className={cn(
        "transition-colors [&>td]:border-b [&>td]:border-border",
        "data-[state=selected]:bg-accent-soft",
        className,
      )}
      {...props}
    />
  );
}

export function TableHead({ className, ...props }: React.ComponentProps<"th">) {
  return (
    <th
      className={cn(
        "h-8 px-3 text-left align-middle text-xs font-medium whitespace-nowrap",
        "tracking-caps text-text-muted uppercase",
        className,
      )}
      {...props}
    />
  );
}

export function TableCell({ className, ...props }: React.ComponentProps<"td">) {
  return <td className={cn("h-10 px-3 align-middle tabular-nums", className)} {...props} />;
}

export function TableCaption({ className, ...props }: React.ComponentProps<"caption">) {
  return <caption className={cn("mt-3 text-xs text-text-muted", className)} {...props} />;
}
