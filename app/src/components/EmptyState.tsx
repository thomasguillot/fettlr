import { cn } from "@fettlr/design-system";
import type { RemixiconComponentType } from "@remixicon/react";
import type { ReactNode } from "react";

export function EmptyState({
  icon: Icon,
  title,
  body,
  action,
  className,
}: {
  icon: RemixiconComponentType;
  title: string;
  body: string;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-1 flex-col items-center justify-center gap-3 px-6 py-10 text-center",
        "scale-100 opacity-100 transition-[opacity,scale]",
        "duration-[var(--duration-base)] ease-[var(--ease-out)]",
        "starting:scale-[0.98] starting:opacity-0",
        className,
      )}
    >
      <span
        aria-hidden="true"
        className="flex size-10 items-center justify-center rounded-full bg-accent-soft"
      >
        <Icon className="size-5 text-accent-text" />
      </span>
      <div className="flex flex-col items-center gap-1">
        <p className="text-lg font-semibold tracking-tight text-text">{title}</p>
        <p className="max-w-prose text-sm text-text-muted">{body}</p>
      </div>
      {action ? <div className="mt-1">{action}</div> : null}
    </div>
  );
}
