import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import type * as React from "react";

import { cn, focusRing } from "../lib/utils";

const buttonVariants = cva(
  cn(
    "inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-md",
    "font-medium select-none",
    // Tailwind's scale utility sets the `scale` property, so `transition-colors` would not carry
    // the press. The default 120ms sits inside the press-feedback band and keeps hover as it was.
    "transition-[color,background-color,border-color,text-decoration-color,fill,stroke,scale]",
    "active:scale-[0.97]",
    "disabled:pointer-events-none disabled:opacity-45",
    "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
    focusRing,
  ),
  {
    variants: {
      variant: {
        primary: "bg-accent text-accent-contrast hover:bg-accent-hover",
        secondary:
          "border border-border-strong bg-surface text-text hover:bg-surface-hover",
        ghost: "text-text hover:bg-surface-hover",
        danger: "bg-danger text-danger-contrast hover:bg-danger-hover",
        dangerGhost:
          "border border-danger-text text-danger-text hover:bg-danger-soft",
        link: "text-accent-text underline-offset-4 hover:underline",
      },
      size: {
        sm: "h-7 px-2.5 text-xs",
        md: "h-9 px-3 text-sm",
        lg: "h-10 px-4 text-base",
        // 3% of 36px is under a pixel and cannot be seen. The dip is sized so the movement
        // matches a full-width button rather than the ratio.
        icon: "size-9 active:scale-[0.92]",
      },
    },
    defaultVariants: {
      variant: "primary",
      size: "md",
    },
  },
);

export type ButtonProps = React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & { asChild?: boolean };

export function Button({ className, variant, size, asChild = false, ...props }: ButtonProps) {
  const Component = asChild ? Slot.Root : "button";
  return (
    <Component
      data-slot="button"
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  );
}

export { buttonVariants };
