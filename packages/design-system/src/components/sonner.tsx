import type * as React from "react";
import { Toaster as SonnerToaster, toast } from "sonner";
import "sonner/dist/styles.css";

export function Toaster({
  position = "bottom-right",
  richColors = false,
  ...props
}: React.ComponentProps<typeof SonnerToaster>) {
  return (
    <SonnerToaster
      position={position}
      richColors={richColors}
      toastOptions={{
        classNames: {
          toast: "font-sans! text-base! shadow-none!",
          // richColors is off so every toast keeps the monochrome --normal-* surface. Recolouring
          // the error toast's root is what tints its icon and title, since both take currentColor.
          error: "text-danger-text!",
          title: "font-medium!",
          description: "text-sm! text-text-muted!",
          actionButton: "bg-accent! text-accent-contrast! text-sm! font-medium!",
          cancelButton: "bg-bg-subtle! text-text-muted! text-sm! font-medium!",
        },
      }}
      style={
        {
          "--normal-bg": "var(--color-surface)",
          "--normal-text": "var(--color-text)",
          "--normal-border": "var(--color-border)",
          "--success-bg": "var(--color-success-soft)",
          "--success-text": "var(--color-success-text)",
          "--success-border": "var(--color-border)",
          "--error-bg": "var(--color-danger-soft)",
          "--error-text": "var(--color-danger-text)",
          "--error-border": "var(--color-border)",
          "--warning-bg": "var(--color-warning-soft)",
          "--warning-text": "var(--color-warning-text)",
          "--warning-border": "var(--color-border)",
          "--info-bg": "var(--color-accent-soft)",
          "--info-text": "var(--color-accent-text)",
          "--info-border": "var(--color-border)",
          "--border-radius": "var(--radius-lg)",
        } as React.CSSProperties
      }
      {...props}
    />
  );
}

export { toast };
