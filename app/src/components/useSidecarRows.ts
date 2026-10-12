import { useRef, type KeyboardEvent, type MouseEvent } from "react";

interface SidecarPane {
  id: string | null;
  open: boolean;
  select: (id: string) => void;
  close: () => void;
}

export function useSidecarRows(
  pane: SidecarPane,
  rowIds: string[],
  options: { active?: boolean; fallbackFocus?: () => void } = {},
) {
  const { active = true, fallbackFocus } = options;
  const bodyRef = useRef<HTMLTableSectionElement>(null);

  const open = active && pane.open && pane.id !== null && rowIds.includes(pane.id);
  const selectedId = open ? pane.id : null;
  const remembered = active && pane.id !== null && rowIds.includes(pane.id) ? pane.id : null;
  const tabStopId = remembered ?? rowIds[0] ?? null;

  const focusRow = (id: string | null): boolean => {
    if (!id || !bodyRef.current) return false;
    for (const row of Array.from(bodyRef.current.rows)) {
      if (row.dataset.rowId === id) {
        row.focus();
        return true;
      }
    }
    return false;
  };

  const closeDetail = () => {
    if (!focusRow(pane.id)) {
      const index = pane.id === null ? -1 : rowIds.indexOf(pane.id);
      const fallback = rowIds[index - 1] ?? rowIds[index + 1] ?? null;
      if (fallback) focusRow(fallback);
      else fallbackFocus?.();
    }
    pane.close();
  };

  const rowProps = (id: string) => {
    const selected = id === selectedId;

    // WebKit does not focus a tabindex row on click, so the row would answer no key press until
    // the user tabbed to it.
    const onClick = (event: MouseEvent<HTMLTableRowElement>) => {
      event.currentTarget.focus();
      pane.select(id);
    };

    // The guard keeps arrow, space and enter presses that belong to a switch or a button inside a
    // row from being swallowed by the row itself.
    const onKeyDown = (event: KeyboardEvent<HTMLTableRowElement>) => {
      if (event.target !== event.currentTarget) return;
      const index = rowIds.indexOf(id);
      const moveTo = (nextIndex: number) => {
        const target = rowIds[nextIndex];
        if (target === undefined) return;
        event.preventDefault();
        focusRow(target);
        pane.select(target);
      };
      switch (event.key) {
        case "ArrowDown":
          moveTo(index + 1);
          break;
        case "ArrowUp":
          moveTo(index - 1);
          break;
        case "Home":
          moveTo(0);
          break;
        case "End":
          moveTo(rowIds.length - 1);
          break;
        case "Enter":
        case " ":
          event.preventDefault();
          pane.select(id);
          break;
        case "Escape":
          event.preventDefault();
          pane.close();
          break;
        default:
          break;
      }
    };

    return {
      "data-row-id": id,
      "data-state": selected ? "selected" : undefined,
      "aria-selected": selected,
      tabIndex: id === tabStopId ? 0 : -1,
      onClick,
      onKeyDown,
    };
  };

  return { bodyRef, open, selectedId, closeDetail, rowProps };
}
