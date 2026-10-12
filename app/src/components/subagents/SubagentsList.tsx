import { Badge, cn } from "@fettlr/design-system";
import { RiFileDamageLine, RiRobot2Line } from "@remixicon/react";
import { useTranslation } from "react-i18next";

import type { UiParkedSubagent, UiSubagent } from "../../bindings";
import { useFettlr } from "../../store";
import { FilterInput } from "../FilterInput";
import { FilterMenu } from "../FilterMenu";
import { ScreenShell, SidecarLayout, capsHeading, contentFrame, screenColumn } from "../screen";
import { parkedSubagentSearchValue, subagentSearchValue } from "../searchText";
import { ModelBadge, ProvenanceBadge, ScopeBadge } from "../badges";
import { SubagentDetail } from "./SubagentDetail";
import { EmptyState } from "../EmptyState";
import { OptionRow, ParkableListbox, type OptionProps } from "../parkable/ParkableListbox";
import { useParkableList, type SourceKind } from "../parkable/useParkableList";

const rowGrid =
  "grid items-center gap-3 px-4 grid-cols-[minmax(0,48fr)_minmax(0,14fr)_minmax(0,16fr)_minmax(0,22fr)]";

const EMPTY_SUBAGENTS: UiSubagent[] = [];
const EMPTY_PARKED: UiParkedSubagent[] = [];

const SOURCE_KINDS: readonly SourceKind[] = [
  { kind: "local", labelKey: "skills.provenanceLocal" },
];

function subagentSortKey(subagent: UiSubagent): string {
  return subagent.name;
}

export function SubagentsList() {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);

  const subagents = snapshot?.subagents ?? EMPTY_SUBAGENTS;
  const parkedSubagents = snapshot?.parkedSubagents ?? EMPTY_PARKED;

  const list = useParkableList({
    section: "subagents",
    kind: "subagent",
    items: subagents,
    parked: parkedSubagents,
    projects: snapshot?.projects,
    plugins: snapshot?.plugins,
    sourceKinds: SOURCE_KINDS,
    scopeColumnLabel: t("subagents.columnScope"),
    sourceColumnLabel: t("subagents.columnSource"),
    activeSearchValue: subagentSearchValue,
    activeSortKey: subagentSortKey,
    parkedSearchValue: parkedSubagentSearchValue,
  });

  return (
    <ScreenShell
      toolbar={
        <>
          <FilterMenu facets={list.facets} />
          <FilterInput
            value={list.query}
            onChange={list.setQuery}
            placeholder={t("filters.placeholderSubagents")}
          />
        </>
      }
    >
      <SidecarLayout
        open={list.open}
        label={t("subagents.detail.paneLabel")}
        onClose={list.closeDetail}
        detail={<SubagentDetail selection={list.storedSelection} onRemoved={list.closeDetail} />}
      >
        <div className={screenColumn}>
          {list.noResults ? (
            <EmptyState
              icon={list.nothingExists ? RiRobot2Line : RiFileDamageLine}
              title={list.nothingExists ? t("subagents.empty") : t("filters.noResults")}
              body={list.nothingExists ? t("subagents.emptyBody") : t("filters.noResultsBody")}
            />
          ) : (
            <div className={contentFrame}>
              <div
                aria-hidden="true"
                className={cn(rowGrid, "h-8 shrink-0 border-b border-border bg-bg-subtle", capsHeading)}
              >
                <span className="truncate">{t("subagents.columnName")}</span>
                <span className="truncate">{t("subagents.columnModel")}</span>
                <span className="truncate">{t("subagents.columnScope")}</span>
                <span className="truncate">{t("subagents.columnSource")}</span>
              </div>
              <ParkableListbox
                listRef={list.listRef}
                label={t("subagents.listLabel")}
                activeDomId={list.activeDomId}
                onKeyDown={list.handleListKeyDown}
                parkedHeading={t("subagents.parkedHeading")}
                parkedRows={list.filteredParked.map((parked) => (
                  <ParkedSubagentRow
                    key={parked.id}
                    parked={parked}
                    {...list.optionProps("parked", parked.id)}
                  />
                ))}
              >
                {list.filteredItems.map((subagent) => (
                  <SubagentRow
                    key={subagent.id}
                    subagent={subagent}
                    {...list.optionProps("subagent", subagent.id)}
                  />
                ))}
              </ParkableListbox>
            </div>
          )}
        </div>
      </SidecarLayout>
    </ScreenShell>
  );
}

function SubagentRow({ subagent, ...option }: OptionProps & { subagent: UiSubagent }) {
  return (
    <OptionRow gridClassName={rowGrid} {...option}>
      <span className="flex min-w-0 items-center gap-2">
        <span className="truncate text-sm font-medium text-text">{subagent.name}</span>
        {subagent.summary ? (
          <span className="min-w-0 flex-1 truncate text-sm text-text-muted">{subagent.summary}</span>
        ) : null}
      </span>
      <div className="flex min-w-0 overflow-hidden">
        <ModelBadge model={subagent.model} />
      </div>
      <div className="flex min-w-0 overflow-hidden">
        <ScopeBadge scope={subagent.scope} />
      </div>
      <div className="flex min-w-0 overflow-hidden">
        <ProvenanceBadge provenance={subagent.provenance} />
      </div>
    </OptionRow>
  );
}

function ParkedSubagentRow({ parked, ...option }: OptionProps & { parked: UiParkedSubagent }) {
  const { t } = useTranslation();
  return (
    <OptionRow gridClassName={rowGrid} {...option}>
      <span className="flex min-w-0 items-center gap-2">
        <span
          className={cn(
            "truncate text-sm",
            option.selected ? "font-medium text-text" : "text-text-muted",
          )}
        >
          {parked.name}
        </span>
      </span>
      <div className="flex min-w-0 overflow-hidden">
        <Badge tone="neutral" variant="soft" className="shrink-0">
          {t("subagents.parkedBadge")}
        </Badge>
      </div>
      <div aria-hidden="true" />
      <div aria-hidden="true" />
    </OptionRow>
  );
}
