import { Badge, cn } from "@fettlr/design-system";
import { RiFileDamageLine, RiSparklingLine } from "@remixicon/react";
import { useTranslation } from "react-i18next";

import type { UiParkedSkill, UiSkill, UiUpdateReport, UiUpdateState } from "../../bindings";
import { useFettlr } from "../../store";
import { describeUpdateState } from "../../updateState";
import { FilterInput } from "../FilterInput";
import { FilterMenu } from "../FilterMenu";
import { ScreenShell, SidecarLayout, capsHeading, contentFrame, screenColumn } from "../screen";
import { parkedSearchValue, skillSearchValue } from "../searchText";
import { ProvenanceBadge, ScopeBadge, StaleBadge } from "../badges";
import { SkillDetail } from "./SkillDetail";
import { EmptyState } from "../EmptyState";
import { OptionRow, ParkableListbox, type OptionProps } from "../parkable/ParkableListbox";
import { useParkableList, type SourceKind } from "../parkable/useParkableList";

const rowGrid =
  "grid items-center gap-3 px-4 grid-cols-[minmax(0,58fr)_minmax(0,18fr)_minmax(0,24fr)]";

const EMPTY_SKILLS: UiSkill[] = [];
const EMPTY_PARKED: UiParkedSkill[] = [];

const SOURCE_KINDS: readonly SourceKind[] = [
  { kind: "local", labelKey: "skills.provenanceLocal" },
  { kind: "gitClone", labelKey: "skills.provenanceGit" },
  { kind: "skillsCli", labelKey: "skills.provenanceSkillsCli" },
];

function skillUpdateState(
  report: UiUpdateReport | null | undefined,
  skillId: string,
): UiUpdateState | undefined {
  return report?.skills[skillId];
}

function skillSortKey(skill: UiSkill): string {
  return skill.title ?? skill.name;
}

export function SkillsList() {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);

  const skills = snapshot?.skills ?? EMPTY_SKILLS;
  const parkedSkills = snapshot?.parkedSkills ?? EMPTY_PARKED;
  const updateReport = snapshot?.updateReport ?? null;

  const list = useParkableList({
    section: "skills",
    kind: "skill",
    items: skills,
    parked: parkedSkills,
    projects: snapshot?.projects,
    plugins: snapshot?.plugins,
    sourceKinds: SOURCE_KINDS,
    scopeColumnLabel: t("skills.columnScope"),
    sourceColumnLabel: t("skills.columnSource"),
    activeSearchValue: skillSearchValue,
    activeSortKey: skillSortKey,
    parkedSearchValue,
  });

  return (
    <ScreenShell
      toolbar={
        <>
          <FilterMenu facets={list.facets} />
          <FilterInput
            value={list.query}
            onChange={list.setQuery}
            placeholder={t("filters.placeholderSkills")}
          />
        </>
      }
    >
      <SidecarLayout
        open={list.open}
        label={t("skills.detail.paneLabel")}
        onClose={list.closeDetail}
        detail={<SkillDetail selection={list.storedSelection} onRemoved={list.closeDetail} />}
      >
        <div className={screenColumn}>
          {list.noResults ? (
            <EmptyState
              icon={list.nothingExists ? RiSparklingLine : RiFileDamageLine}
              title={list.nothingExists ? t("skills.empty") : t("filters.noResults")}
              body={list.nothingExists ? t("skills.emptyBody") : t("filters.noResultsBody")}
            />
          ) : (
            <div className={contentFrame}>
              <div
                aria-hidden="true"
                className={cn(rowGrid, "h-8 shrink-0 border-b border-border bg-bg-subtle", capsHeading)}
              >
                <span className="truncate">{t("skills.columnName")}</span>
                <span className="truncate">{t("skills.columnScope")}</span>
                <span className="truncate">{t("skills.columnSource")}</span>
              </div>
              <ParkableListbox
                listRef={list.listRef}
                label={t("skills.listLabel")}
                activeDomId={list.activeDomId}
                onKeyDown={list.handleListKeyDown}
                parkedHeading={t("skills.parkedHeading")}
                parkedRows={list.filteredParked.map((parked) => (
                  <ParkedSkillRow
                    key={parked.id}
                    parked={parked}
                    {...list.optionProps("parked", parked.id)}
                  />
                ))}
              >
                {list.filteredItems.map((skill) => (
                  <SkillRow
                    key={skill.id}
                    skill={skill}
                    state={skillUpdateState(updateReport, skill.id)}
                    {...list.optionProps("skill", skill.id)}
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

function SkillRow({
  skill,
  state,
  ...option
}: OptionProps & {
  skill: UiSkill;
  state: UiUpdateState | undefined;
}) {
  const { t } = useTranslation();
  const pending = describeUpdateState(state, t);
  return (
    <OptionRow gridClassName={rowGrid} {...option}>
      <span className="flex min-w-0 items-center gap-2">
        <span className="truncate text-sm font-medium text-text">
          {skill.title ?? skill.name}
        </span>
        {skill.summary ? (
          <span className="min-w-0 flex-1 truncate text-sm text-text-muted">{skill.summary}</span>
        ) : null}
        {pending ? <StaleBadge>{pending}</StaleBadge> : null}
      </span>
      <div className="flex min-w-0 overflow-hidden">
        <ScopeBadge scope={skill.scope} />
      </div>
      <div className="flex min-w-0 overflow-hidden">
        <ProvenanceBadge provenance={skill.provenance} />
      </div>
    </OptionRow>
  );
}

function ParkedSkillRow({ parked, ...option }: OptionProps & { parked: UiParkedSkill }) {
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
        <Badge tone="neutral" variant="soft" className="shrink-0">
          {t("skills.parkedBadge")}
        </Badge>
      </span>
      <span aria-hidden="true" />
      <span aria-hidden="true" />
    </OptionRow>
  );
}
