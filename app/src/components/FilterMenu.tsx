import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  cn,
} from "@fettlr/design-system";
import { RiFilter3Line } from "@remixicon/react";
import { useTranslation } from "react-i18next";

export const ANY = "any";

export type FilterOption = { value: string; label: string };

export type FilterFacet = {
  id: string;
  label: string;
  anyLabel: string;
  value: string;
  options: FilterOption[];
  onChange: (value: string) => void;
};

// A facet with nothing to choose between is noise: one option plus "any" is not a filter.
function isUseful(facet: FilterFacet): boolean {
  return facet.options.length > 1 || (facet.options.length === 1 && facet.value !== ANY);
}

export function FilterMenu({ facets }: { facets: FilterFacet[] }) {
  const { t } = useTranslation();
  const usable = facets.filter(isUseful);
  if (usable.length === 0) return null;

  const active = usable.filter((facet) => facet.value !== ANY);
  const summary =
    active.length === 1
      ? (active[0].options.find((option) => option.value === active[0].value)?.label ??
        t("filters.trigger"))
      : active.length > 1
        ? t("filters.activeCount", { count: active.length })
        : t("filters.trigger");

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="secondary"
          aria-label={t("filters.triggerAria", { summary })}
          className={cn("max-w-52", active.length > 0 && "text-accent-text")}
        >
          <RiFilter3Line className="size-4 shrink-0" aria-hidden="true" />
          <span className="truncate">{summary}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="min-w-52">
        {usable.map((facet, index) => (
          <div key={facet.id}>
            {index > 0 ? <DropdownMenuSeparator /> : null}
            <DropdownMenuLabel className="tracking-caps uppercase">{facet.label}</DropdownMenuLabel>
            <DropdownMenuRadioGroup value={facet.value} onValueChange={facet.onChange}>
              <DropdownMenuRadioItem value={ANY}>{facet.anyLabel}</DropdownMenuRadioItem>
              {facet.options.map((option) => (
                <DropdownMenuRadioItem key={option.value} value={option.value}>
                  <span className="truncate">{option.label}</span>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </div>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
