import { cva, type VariantProps } from "class-variance-authority";
import { Dialog as SheetPrimitive } from "radix-ui";
import type * as React from "react";

import { cn } from "../lib/utils";
import { CloseLabelContext, HeaderClose } from "./dialog";

export const Sheet = SheetPrimitive.Root;
export const SheetTrigger = SheetPrimitive.Trigger;
export const SheetClose = SheetPrimitive.Close;

const sheetVariants = cva(
  cn("fixed z-50 flex flex-col gap-4 border-border bg-surface text-text outline-none"),
  {
    variants: {
      side: {
        right:
          "inset-y-0 right-0 h-full w-3/4 max-w-md border-l data-[state=open]:animate-slide-in-right data-[state=closed]:animate-slide-out-right",
        left: "inset-y-0 left-0 h-full w-3/4 max-w-md border-r data-[state=open]:animate-slide-in-left data-[state=closed]:animate-slide-out-left",
        top: "inset-x-0 top-0 border-b data-[state=open]:animate-slide-in-top data-[state=closed]:animate-slide-out-top",
        bottom:
          "inset-x-0 bottom-0 border-t data-[state=open]:animate-slide-in-bottom data-[state=closed]:animate-slide-out-bottom",
      },
    },
    defaultVariants: {
      side: "right",
    },
  },
);

export function SheetContent({
  className,
  children,
  side,
  closeLabel = "Close",
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Content> &
  VariantProps<typeof sheetVariants> & { closeLabel?: string }) {
  return (
    <SheetPrimitive.Portal>
      <SheetPrimitive.Overlay className="fixed inset-0 z-50" />
      <SheetPrimitive.Content
        data-slot="sheet-content"
        className={cn(sheetVariants({ side }), className)}
        {...props}
      >
        <CloseLabelContext.Provider value={closeLabel}>{children}</CloseLabelContext.Provider>
      </SheetPrimitive.Content>
    </SheetPrimitive.Portal>
  );
}

export function SheetHeader({ className, children, ...props }: React.ComponentProps<"div">) {
  return (
    <div className={cn("flex items-start justify-between gap-4 border-b border-border px-5 pt-5 pb-4", className)} {...props}>
      <div className="flex min-w-0 flex-col gap-1">{children}</div>
      <HeaderClose />
    </div>
  );
}

export function SheetBody({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("flex min-h-0 flex-1 flex-col gap-4 px-5", className)} {...props} />;
}

export function SheetFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      className={cn("mt-auto flex items-center justify-end gap-2 px-5 pt-4 pb-5", className)}
      {...props}
    />
  );
}

export function SheetTitle({
  className,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Title>) {
  return (
    <SheetPrimitive.Title
      className={cn("text-lg font-semibold tracking-tight", className)}
      {...props}
    />
  );
}

export function SheetDescription({
  className,
  ...props
}: React.ComponentProps<typeof SheetPrimitive.Description>) {
  return (
    <SheetPrimitive.Description className={cn("text-sm text-text-muted", className)} {...props} />
  );
}
