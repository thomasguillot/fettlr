import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export const focusRing =
  "focus-visible:outline-2 focus-visible:outline-solid focus-visible:outline-offset-2 " +
  "focus-visible:outline-ring";

// Popper surfaces grow out of the control that opened them, so the scale runs from Radix's
// measured origin rather than the panel's own centre.
export const popoverSurface =
  "z-50 rounded-lg border border-border bg-surface text-text " +
  "origin-[var(--radix-popper-transform-origin)] " +
  "data-[state=open]:animate-pop-in data-[state=closed]:animate-pop-out";

export const insetFocusRing =
  "focus-visible:outline-2 focus-visible:outline-solid focus-visible:-outline-offset-2 " +
  "focus-visible:outline-ring";

export const menuItem =
  "relative flex cursor-default select-none items-center gap-2 rounded-sm px-2 py-1.5 text-sm " +
  "outline-none transition-colors data-[disabled]:pointer-events-none data-[disabled]:opacity-45 " +
  "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 " +
  insetFocusRing;
