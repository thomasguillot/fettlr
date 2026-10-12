import { ScrollArea, cn, insetFocusRing } from "@fettlr/design-system";
import type { KeyboardEvent, ReactNode, Ref } from "react";

import { capsHeading, groupHeading } from "../screen";

export type OptionProps = {
  selected: boolean;
  optionId: string;
  optionRef: (el: HTMLLIElement | null) => void;
  onSelect: () => void;
};

export function OptionRow({
  gridClassName,
  selected,
  optionId,
  optionRef,
  onSelect,
  children,
}: OptionProps & { gridClassName: string; children: ReactNode }) {
  return (
    <li
      ref={optionRef}
      id={optionId}
      role="option"
      aria-selected={selected}
      onClick={onSelect}
      className={cn(
        gridClassName,
        "min-h-10 border-b border-border py-2 text-left transition-colors",
        selected ? "bg-accent-soft" : "hover:bg-bg-subtle",
      )}
    >
      {children}
    </li>
  );
}

export function ParkableListbox({
  listRef,
  label,
  activeDomId,
  onKeyDown,
  parkedHeading,
  parkedRows,
  children,
}: {
  listRef: Ref<HTMLUListElement>;
  label: string;
  activeDomId: string | undefined;
  onKeyDown: (event: KeyboardEvent<HTMLUListElement>) => void;
  parkedHeading: string;
  parkedRows: ReactNode[];
  children: ReactNode;
}) {
  return (
    <ScrollArea className="min-h-0">
      <ul
        ref={listRef}
        role="listbox"
        aria-label={label}
        aria-activedescendant={activeDomId}
        tabIndex={0}
        onKeyDown={onKeyDown}
        className={cn("flex flex-col", insetFocusRing)}
      >
        {children}
        {parkedRows.length > 0 ? (
          <>
            <li role="presentation" className={cn(groupHeading, capsHeading)}>
              {parkedHeading}
            </li>
            {parkedRows}
          </>
        ) : null}
      </ul>
    </ScrollArea>
  );
}
