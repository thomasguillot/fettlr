import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";

import { cn } from "../lib/utils";

const badgeVariants = cva(
  cn(
    "inline-flex h-5.5 items-center gap-1 rounded-full px-2 whitespace-nowrap",
    "text-xs font-medium",
    "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3",
  ),
  {
    variants: {
      tone: {
        neutral: "",
        accent: "",
        blue: "",
        success: "",
        warning: "",
        danger: "",
      },
      variant: {
        soft: "",
        solid: "",
        outline: "border border-border bg-transparent text-text-muted",
      },
    },
    compoundVariants: [
      { tone: "neutral", variant: "soft", class: "bg-bg-subtle text-text-muted" },
      { tone: "neutral", variant: "solid", class: "bg-inverse text-inverse-contrast" },
      { tone: "accent", variant: "soft", class: "bg-accent-soft text-accent-text" },
      { tone: "accent", variant: "solid", class: "bg-accent text-accent-contrast" },
      { tone: "blue", variant: "soft", class: "bg-blue-soft text-blue-text" },
      { tone: "blue", variant: "solid", class: "bg-blue text-blue-contrast" },
      { tone: "success", variant: "soft", class: "bg-success-soft text-success-text" },
      { tone: "success", variant: "solid", class: "bg-success text-success-contrast" },
      { tone: "warning", variant: "soft", class: "bg-warning-soft text-warning-text" },
      { tone: "warning", variant: "solid", class: "bg-warning text-warning-contrast" },
      { tone: "danger", variant: "soft", class: "bg-danger-soft text-danger-text" },
      { tone: "danger", variant: "solid", class: "bg-danger text-danger-contrast" },
    ],
    defaultVariants: {
      tone: "neutral",
      variant: "soft",
    },
  },
);

export type BadgeProps = React.ComponentProps<"span"> & VariantProps<typeof badgeVariants>;

export function Badge({ className, tone, variant, ...props }: BadgeProps) {
  return (
    <span
      data-slot="badge"
      className={cn(badgeVariants({ tone, variant }), className)}
      {...props}
    />
  );
}

export { badgeVariants };
