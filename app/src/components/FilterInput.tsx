import { Input } from "@fettlr/design-system";
import { RiSearchLine } from "@remixicon/react";
import { useRef, type KeyboardEvent } from "react";

import { useFindRequest } from "../store";

export function FilterInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useFindRequest(() => {
    ref.current?.focus();
    ref.current?.select();
  });

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key !== "Escape") return;
    event.preventDefault();
    if (value === "") event.currentTarget.blur();
    else onChange("");
  }

  return (
    <div className="relative w-56 shrink-0">
      <RiSearchLine
        className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-text-muted"
        aria-hidden="true"
      />
      <Input
        ref={ref}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        aria-label={placeholder}
        autoComplete="off"
        spellCheck={false}
        className="pl-8"
      />
    </div>
  );
}
