import { Button, ScrollArea, cn } from "@fettlr/design-system";
import { RiAlertLine, RiCloseLine, type RemixiconComponentType } from "@remixicon/react";
import {
  createContext,
  useContext,
  type CSSProperties,
  type ReactNode,
} from "react";
import i18next from "i18next";
import { useTranslation } from "react-i18next";

import type { UiUsage } from "../bindings";
import { useFettlr } from "../store";

const sidecarWidth = { "--sidecar-width": "min(24rem, 34vw)" } as CSSProperties;

export const framedTable =
  "table-fixed [&_th:first-child]:pl-4 [&_td:first-child]:pl-4 " +
  "[&_th:last-child]:pr-4 [&_td:last-child]:pr-4";

export const contentFrame =
  "flex min-h-0 flex-col overflow-hidden rounded-xl border border-border [&>*:last-child]:-mb-px";

export const screenColumn = "flex min-h-0 flex-1 flex-col px-6 pb-6";

export const stickyTableHeader = "sticky top-0 z-10";

// The sticky header outranks the overlay scrollbar, so the top of the thumb would sit behind it.
// Inset by the header's own h-8 so the whole track stays visible, as it already is on Skills,
// where the header is a sibling above the scroll area rather than a row inside it. Radix sets
// `top` inline, hence the important.
// DetailHeader is h-12 and sticks inside the pane's own scroll area, so it hides the top of that
// scrollbar exactly as a table header would.
const detailScrollArea =
  "[&_[data-orientation=vertical]]:top-12! [&_[data-orientation=vertical]]:h-[calc(100%-3rem)]";

export const stickyHeaderScrollArea =
  "[&_[data-orientation=vertical]]:top-8! [&_[data-orientation=vertical]]:h-[calc(100%-2rem)]";

export const capsHeading = "text-xs font-medium tracking-caps text-text-muted uppercase";

export const groupHeading = "h-8 border-b border-border bg-bg-subtle px-4 py-1.5";

export const codeChip =
  "block truncate rounded bg-bg-subtle px-2 py-1 font-mono text-xs text-text-muted";

export function DetailRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <dt className={capsHeading}>{label}</dt>
      <dd className="text-sm text-text">{children}</dd>
    </div>
  );
}

export function DetailEmpty({ icon: Icon, message }: { icon: RemixiconComponentType; message: string }) {
  return (
    <div className="flex h-full flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
      <Icon className="size-8 text-text-muted" aria-hidden="true" />
      <p className="max-w-prose text-sm text-text-muted">{message}</p>
    </div>
  );
}

export function formatTimestamp(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat(i18next.language, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

export function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return new Intl.DateTimeFormat(i18next.language, { dateStyle: "medium" }).format(date);
}

// A recorded zero and no record at all are different answers and are never collapsed: zero means
// installed and never reached for, which is the finding, while absence means Claude Code has
// written nothing about it and the pane has no business implying disuse.
export function UsageRow({ usage }: { usage: UiUsage | null }) {
  const { t } = useTranslation();

  if (!usage) {
    return (
      <DetailRow label={t("common.usage")}>
        <span className="text-text-muted">{t("common.usageUnrecorded")}</span>
      </DetailRow>
    );
  }
  if (usage.count === 0) {
    return <DetailRow label={t("common.usage")}>{t("common.usageNever")}</DetailRow>;
  }
  return (
    <DetailRow label={t("common.usage")}>
      {usage.lastUsedAt
        ? t("common.usageCountSince", { count: usage.count, when: formatDate(usage.lastUsedAt) })
        : t("common.usageCount", { count: usage.count })}
    </DetailRow>
  );
}

const SidecarCloseContext = createContext<(() => void) | null>(null);

// The pane's title belongs to whichever detail is showing, so the header is rendered from inside
// the scrolled content and stuck to the top rather than sitting in a bar of its own. The negative
// margin lets its rule reach the pane's edges through the container's padding.
export function DetailHeader({ title }: { title: ReactNode }) {
  const { t } = useTranslation();
  const onClose = useContext(SidecarCloseContext);
  return (
    <div
      className={cn(
        "sticky top-0 z-10 -mx-6 flex h-12 shrink-0 items-center justify-between gap-2",
        "border-b border-border bg-surface px-6",
      )}
    >
      <h2 className="truncate text-lg font-semibold tracking-tight text-text">{title}</h2>
      {onClose ? (
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="-mr-2.5 shrink-0"
          aria-label={t("common.closeDetails")}
          onClick={onClose}
        >
          <RiCloseLine className="size-4" aria-hidden="true" />
        </Button>
      ) : null}
    </div>
  );
}

export function ScreenShell({
  toolbar,
  actions,
  children,
}: {
  toolbar?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
}) {
  // The strip stays even with nothing in it: the window has no system title bar, so this is what
  // makes the right of it draggable.
  const bare = !toolbar && !actions;
  return (
    <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-bg">
      <div
        data-tauri-drag-region
        className={cn(
          "flex shrink-0 items-center justify-between gap-2 px-6",
          // Kept only as a drag target once it is empty.
          bare ? "h-5" : "h-15",
        )}
      >
        <div className="flex h-full min-w-0 flex-1 items-center gap-2">{toolbar}</div>
        {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
      </div>
      <CoreWarnings />
      {children}
    </div>
  );
}

// Every degraded path in the core records a warning and carries on with an empty list. Shown on
// every screen rather than on one landing screen alone: the screen a reader is on when a list
// comes back empty is the screen that has to say why, or "no skills found yet" is simply untrue.
function CoreWarnings() {
  const { t } = useTranslation();
  const warnings = useFettlr((state) => state.snapshot?.warnings ?? []);
  if (warnings.length === 0) return null;
  return (
    <div className="shrink-0 border-b border-border bg-bg-subtle px-6 py-3">
      <p className="flex items-center gap-2 text-sm font-medium text-text">
        <RiAlertLine className="size-4 shrink-0 text-warning-text" aria-hidden="true" />
        {t("warnings.title", { count: warnings.length })}
      </p>
      <p className="mt-1 text-xs text-text-muted">{t("warnings.body")}</p>
      <ul className="mt-1 flex flex-col gap-1 text-xs break-words text-text-muted">
        {warnings.map((warning, index) => (
          <li key={`${index}:${warning}`}>{warning}</li>
        ))}
      </ul>
    </div>
  );
}

export function SidecarLayout({
  open,
  label,
  onClose,
  detail,
  children,
}: {
  open: boolean;
  label: string;
  onClose: () => void;
  detail: ReactNode;
  children: ReactNode;
}) {
  return (
    <div style={sidecarWidth} className="min-h-0 flex-1 overflow-hidden">
      <div className="flex h-full min-h-0 min-w-0 flex-col">{children}</div>
      <aside
        aria-label={label}
        aria-hidden={!open}
        inert={!open}
        onKeyDown={(event) => {
          if (event.key !== "Escape") return;
          if (event.defaultPrevented || !event.currentTarget.contains(event.target as Node)) return;
          event.stopPropagation();
          onClose();
        }}
        className={cn(
          // Outranks the sticky table headers below, which sit at z-10.
          "absolute inset-y-0 right-0 z-20 m-3 flex w-[var(--sidecar-width)] flex-col overflow-hidden",
          "rounded-xl border border-border bg-surface shadow-lg",
          // Tailwind's translate utilities set the `translate` property, not `transform`.
          "transition-[translate] duration-[var(--duration-slow)] ease-out",
          // Its own width plus the margin would leave the shadow's 40px blur smudging the edge,
          // so it travels far enough to take the shadow with it and needs no fade to hide it.
          open ? "translate-x-0" : "translate-x-[calc(100%_+_4rem)]",
        )}
      >
        <SidecarCloseContext.Provider value={onClose}>
          <ScrollArea fillViewport className={cn("min-h-0 flex-1", detailScrollArea)}>{detail}</ScrollArea>
        </SidecarCloseContext.Provider>
      </aside>
    </div>
  );
}
