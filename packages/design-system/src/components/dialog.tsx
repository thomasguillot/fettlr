import { RiCloseLine } from "@remixicon/react";
import { Dialog as DialogPrimitive } from "radix-ui";
import { createContext, useContext } from "react";
import type * as React from "react";

import { cn, focusRing } from "../lib/utils";

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;

export const CloseLabelContext = createContext("Close");

// Sits in the header's row rather than floating over it, so the glyph lines up with the title's
// line box instead of landing wherever an absolute offset happens to put it. The negative margins
// hang the button's own padding into the surrounding gutter, which aligns the glyph, not the box.
export function HeaderClose() {
  const closeLabel = useContext(CloseLabelContext);
  return (
    <DialogPrimitive.Close
      className={cn(
        "-mt-px -mr-1.5 inline-flex size-7 shrink-0 items-center justify-center",
        "rounded-md text-text-muted transition-colors hover:bg-surface-hover hover:text-text",
        focusRing,
      )}
    >
      <RiCloseLine className="size-4" />
      <span className="sr-only">{closeLabel}</span>
    </DialogPrimitive.Close>
  );
}

export function DialogOverlay({
  className,
  motion = true,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Overlay> & { motion?: boolean }) {
  return (
    <DialogPrimitive.Overlay
      className={cn(
        "fixed inset-0 z-50 bg-overlay",
        motion && "data-[state=open]:animate-fade-in data-[state=closed]:animate-fade-out",
        className,
      )}
      {...props}
    />
  );
}

export function DialogContent({
  className,
  children,
  closeLabel = "Close",
  motion = true,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Content> & {
  closeLabel?: string;
  /**
   * Off for a surface the reader opens from the keyboard. At hundreds of opens a day an entrance
   * is latency rather than explanation, and the overlay has to go quiet with it or the panel
   * appears against a fading ground.
   */
  motion?: boolean;
}) {
  return (
    <DialogPrimitive.Portal>
      <DialogOverlay motion={motion} />
      {/* Scrolling has to live on a box outside the centring flex: a centred item taller than its
          container overflows past both edges, and neither edge can be scrolled back to. The inset
          feeds both the gap around the dialog and its height cap, so the two cannot drift apart. */}
      <div className="fixed inset-0 z-50 overflow-y-auto overscroll-contain [--dialog-inset:0.5rem]">
        <div className="flex min-h-full items-center justify-center p-[var(--dialog-inset)]">
          <DialogPrimitive.Content
            data-slot="dialog-content"
            className={cn(
              "relative flex w-full max-w-lg flex-col rounded-xl border border-border",
              "max-h-[calc(100vh-var(--dialog-inset)*2)]",
              "bg-surface text-text outline-none",
              motion && "data-[state=open]:animate-pop-in data-[state=closed]:animate-pop-out",
              className,
            )}
            {...props}
          >
            <CloseLabelContext.Provider value={closeLabel}>{children}</CloseLabelContext.Provider>
          </DialogPrimitive.Content>
        </div>
      </div>
    </DialogPrimitive.Portal>
  );
}

export function DialogHeader({ className, children, ...props }: React.ComponentProps<"div">) {
  return (
    <div className={cn("shrink-0 border-b border-border px-5 pt-5 pb-4", "flex items-start justify-between gap-4", className)} {...props}>
      <div className="flex min-w-0 flex-col gap-1">{children}</div>
      <HeaderClose />
    </div>
  );
}

export function DialogBody({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("flex min-h-0 flex-1 flex-col gap-4 px-5 pt-5", className)} {...props} />;
}

export function DialogFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div className={cn("flex items-center justify-end gap-2 px-5 pt-5 pb-5", className)} {...props} />
  );
}

export function DialogTitle({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Title>) {
  return (
    <DialogPrimitive.Title
      className={cn("text-lg font-semibold tracking-tight", className)}
      {...props}
    />
  );
}

export function DialogDescription({
  className,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Description>) {
  return (
    <DialogPrimitive.Description className={cn("text-sm text-text-muted", className)} {...props} />
  );
}
