import {
  Badge,
  Button,
  Separator,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  cn,
} from "@fettlr/design-system";
import {
  RiSidebarFoldLine,
  RiSidebarUnfoldLine,
  RiSettings3Line,
  type RemixiconComponentType,
} from "@remixicon/react";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { needsUpdatingTotal, sectionCount, sections } from "../sections";
import { useFettlr } from "../store";

// The row's contents are held at the width they occupy when the sidebar is open, so the rail closes
// over them instead of reflowing them. Every term is a constant this file owns: the open width, its
// own right border, the column's padding, and the row's padding when open.
export const sidebarWidths = {
  "--sidebar-row": "calc(15rem - 1px - 2.5rem)",
  "--sidebar-row-content": "calc(var(--sidebar-row) - 1.25rem)",
  // Same three terms as the open width, in the other direction: the closed row, the column's
  // padding and the nav's own border. A rail a pixel short leaves the row overflowing a centred
  // column, which walks every icon half a pixel left as the sidebar closes.
  "--sidebar-rail": "calc(2.25rem + 2.5rem + 1px)",
} as CSSProperties;

const ease = "duration-[var(--duration-slow)] ease-[var(--ease-in-out)]";
const fade = `transition-opacity ${ease}`;
const slide = `transition-[translate] ${ease}`;

// The rail's width belongs to the collapse and takes the slow token; colour and the press dip
// answer a pointer and take the fast one. A single transition-property utility cannot hold two
// timings, so both live in styles.css, unlayered, where they outrank the button's own.
const glide = "n-rail-glide";

export function Sidebar() {
  const { t } = useTranslation();
  const selection = useFettlr((state) => state.selection);
  const select = useFettlr((state) => state.select);
  const snapshot = useFettlr((state) => state.snapshot);
  const appUpdate = useFettlr((state) => state.appUpdate);
  const prefs = useFettlr((state) => state.prefs);
  const savePrefs = useFettlr((state) => state.savePrefs);
  const stale = needsUpdatingTotal(snapshot);
  const collapsed = prefs?.sidebarCollapsed ?? false;

  // Rows resize under a stationary cursor, which the browser reports as a hover the reader never
  // performed, and a tooltip opens on its own. Nothing in here is hoverable until it has settled.
  const [moving, setMoving] = useState(false);
  const settled = useRef(collapsed);
  useEffect(() => {
    if (settled.current === collapsed) return;
    settled.current = collapsed;
    setMoving(true);
    const timer = window.setTimeout(() => setMoving(false), 600);
    return () => window.clearTimeout(timer);
  }, [collapsed]);

  return (
    <nav
      aria-label={t("app.name")}
      style={sidebarWidths}
      onTransitionEnd={(event) => {
        if (event.propertyName === "width" && event.target === event.currentTarget) setMoving(false);
      }}
      className={cn(
        "flex shrink-0 flex-col overflow-hidden bg-bg-subtle animate-shell-reveal",
        // Withheld until the stored width is known, so a collapsed sidebar paints collapsed rather
        // than animating in from full width on every launch.
        prefs && `transition-[width] ${ease}`,
        moving && "pointer-events-none",
        collapsed ? "w-(--sidebar-rail)" : "w-60",
      )}
    >
      <div data-tauri-drag-region className="h-7 shrink-0" />

      <div className="flex flex-1 flex-col gap-5 p-5 pb-7">
        <ul className="flex flex-1 flex-col gap-0.5">
          {sections.map((section) => {
            const updates = section.id === "updates";
            const total = updates ? stale : sectionCount(snapshot, section);
            const selected = selection === section.id;
            return (
              <li key={section.id} className="flex justify-center">
                <SidebarRow
                  label={t(section.labelKey)}
                  icon={section.icon}
                  collapsed={collapsed}
                  stale={updates ? stale : 0}
                  selected={selected}
                  onClick={() => select(section.id)}
                  count={(updates || section.count) && total > 0 ? total : undefined}
                  announce={updates && stale > 0 ? t("common.needsUpdatingTotal", { count: stale }) : undefined}
                />
              </li>
            );
          })}
        </ul>

        <div className="flex flex-col gap-2">
          <ul className="flex flex-col gap-0.5">
            <li className="flex justify-center">
              <SidebarRow
                label={t("sections.settings")}
                icon={RiSettings3Line}
                collapsed={collapsed}
                selected={selection === "settings"}
                stale={appUpdate.kind === "available" ? 1 : 0}
                onClick={() => select("settings")}
                trailing={
                  appUpdate.kind === "available" ? (
                    <span className={cn(fade, collapsed && "opacity-0")}>
                      <Badge tone="blue" variant="soft">
                        {t("settings.updateBadge")}
                      </Badge>
                    </span>
                  ) : null
                }
              />
            </li>
          </ul>

          <div className="flex justify-center">
            <Separator
              className={cn(`transition-[width] ${ease}`, collapsed ? "w-9" : "w-(--sidebar-row)")}
            />
          </div>

          <ul className="flex flex-col gap-0.5">
            <li className="flex justify-center">
              <SidebarRow
                label={t(collapsed ? "sidebar.expand" : "sidebar.collapse")}
                icon={collapsed ? RiSidebarUnfoldLine : RiSidebarFoldLine}
                collapsed={collapsed}
                expanded={!collapsed}
                disabled={!prefs}
                onClick={() => {
                  if (!prefs) return;
                  setMoving(true);
                  void savePrefs({ ...prefs, sidebarCollapsed: !prefs.sidebarCollapsed }).then(
                    (ok) => {
                      if (!ok) setMoving(false);
                    },
                  );
                }}
              />
            </li>
          </ul>
        </div>
      </div>
    </nav>
  );
}

function SidebarTooltip({ label, children }: { label: string | null; children: ReactNode }) {
  const hoverable = Boolean(label);
  const [open, setOpen] = useState(false);
  const [wasHoverable, setWasHoverable] = useState(hoverable);
  if (wasHoverable !== hoverable) {
    setWasHoverable(hoverable);
    setOpen(false);
  }
  return (
    <Tooltip open={hoverable && open} onOpenChange={setOpen}>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      {label ? (
        <TooltipContent side="right" sideOffset={28}>
          {label}
        </TooltipContent>
      ) : null}
    </Tooltip>
  );
}

function SidebarRow({
  label,
  icon: Icon,
  collapsed,
  onClick,
  selected = false,
  stale = 0,
  count,
  expanded,
  disabled = false,
  trailing,
  announce,
}: {
  label: string;
  icon: RemixiconComponentType;
  collapsed: boolean;
  onClick: () => void;
  selected?: boolean;
  stale?: number;
  count?: number;
  expanded?: boolean;
  disabled?: boolean;
  trailing?: ReactNode;
  announce?: string;
}) {
  return (
    <SidebarTooltip label={collapsed ? label : null}>
      <Button
        type="button"
        variant="ghost"
        size="md"
        aria-current={selected ? "page" : undefined}
        aria-expanded={expanded}
        disabled={disabled}
        onClick={onClick}
        className={cn(
          "group shrink-0 justify-start overflow-hidden px-2.5 text-text-muted",
          "relative focus-visible:z-10",
          glide,
          collapsed ? "w-9 active:scale-[0.92]" : "w-(--sidebar-row)",
          selected &&
            "bg-accent-soft text-accent-text hover:bg-accent-soft hover:text-accent-text",
        )}
      >
        <span className="flex w-(--sidebar-row-content) shrink-0 items-center gap-2">
          <span className="relative shrink-0">
            <Icon className="size-4" aria-hidden="true" />
            {stale > 0 ? (
              <span
                aria-hidden="true"
                style={count ? ({ "--digits": String(count).length } as CSSProperties) : undefined}
                className={cn(
                  "absolute -top-px -right-px size-1.5 rounded-full bg-blue ring-1",
                  selected
                    ? "ring-accent-soft"
                    : "ring-bg-subtle group-hover:ring-surface-hover",
                  count
                    ? cn(
                        slide,
                        "text-xs font-normal tabular-nums",
                        collapsed
                          ? "[translate:0px_0px]"
                          : "[translate:calc(var(--sidebar-row-content)_-_var(--digits)*1ch_-_23px)_6px]",
                      )
                    : cn(fade, collapsed ? "opacity-100" : "opacity-0"),
                )}
              />
            ) : null}
          </span>
          <span className={cn("flex-1 truncate text-left", fade, collapsed && "opacity-0")}>
            {label}
          </span>
          {count ? (
            <span
              aria-hidden="true"
              className={cn(
                "shrink-0 text-xs font-normal text-text-muted tabular-nums",
                stale > 0 && "pl-3",
                fade,
                collapsed && "opacity-0",
              )}
            >
              {count}
            </span>
          ) : null}
          {trailing}
        </span>
        {announce ? <span className="sr-only">{announce}</span> : null}
      </Button>
    </SidebarTooltip>
  );
}
