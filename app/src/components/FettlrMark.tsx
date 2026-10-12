import { cn } from "@fettlr/design-system";

function FettlrGlyph({ className }: { className?: string }) {
  return (
    <svg viewBox="4 4 16 16" fill="none" className={className} aria-hidden="true">
      <path d="M8 20H6V19H7V12H8V6H9V4H17V5H18V6H13V8H12V9H11V11H15V12H16V13H11V15H10V16H9V19H8V20Z" fill="currentColor" />
    </svg>
  );
}

export function FettlrMark({ className, glyphClassName }: { className?: string; glyphClassName?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "flex shrink-0 items-center justify-center bg-brand text-brand-contrast",
        className,
      )}
    >
      <FettlrGlyph className={glyphClassName} />
    </span>
  );
}
