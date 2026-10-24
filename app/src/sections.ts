import {
  RiArrowUpCircleLine,
  RiPuzzleLine,
  RiGlobalLine,
  RiRobot2Line,
  RiServerLine,
  RiSparklingLine,
  RiStoreLine,
  type RemixiconComponentType,
} from "@remixicon/react";

import type { UiSnapshot, UiUpdateReport } from "./bindings";

export type SectionId =
  | "updates"
  | "skills"
  | "subagents"
  | "plugins"
  | "marketplaces"
  | "mcp"
  | "connectors"
  | "settings";

export type UpdateDomain = "skills" | "plugins" | "marketplaces" | "mcpServers";

export type SectionDef = {
  id: SectionId;
  labelKey: string;
  icon: RemixiconComponentType;
  domain: UpdateDomain | null;
  count: ((snapshot: UiSnapshot) => number) | null;
};

export const sections: SectionDef[] = [
  { id: "updates", labelKey: "sections.updates", icon: RiArrowUpCircleLine, domain: null, count: null },
  {
    id: "skills",
    labelKey: "sections.skills",
    icon: RiSparklingLine,
    domain: "skills",
    count: (snapshot) => snapshot.skills.length + snapshot.parkedSkills.length,
  },
  {
    id: "subagents",
    labelKey: "sections.subagents",
    icon: RiRobot2Line,
    domain: null,
    count: (snapshot) => snapshot.subagents.length + snapshot.parkedSubagents.length,
  },
  {
    id: "plugins",
    labelKey: "sections.plugins",
    icon: RiPuzzleLine,
    domain: "plugins",
    count: (snapshot) => snapshot.plugins.length,
  },
  {
    id: "marketplaces",
    labelKey: "sections.marketplaces",
    icon: RiStoreLine,
    domain: "marketplaces",
    count: (snapshot) => snapshot.marketplaces.length,
  },
  {
    id: "mcp",
    labelKey: "sections.mcp",
    icon: RiServerLine,
    domain: "mcpServers",
    count: (snapshot) => snapshot.mcpServers.length,
  },
  {
    id: "connectors",
    labelKey: "sections.connectors",
    icon: RiGlobalLine,
    domain: null,
    count: (snapshot) =>
      snapshot.connectorsCheckedAt
        ? snapshot.connectors.filter((connector) => connector.state === "connected").length
        : 0,
  },
];

export function sectionCount(
  snapshot: UiSnapshot | null | undefined,
  section: SectionDef,
): number {
  if (!snapshot || !section.count) return 0;
  return section.count(snapshot);
}

// Staleness is the core's call, not this window's: update_all works through exactly the ids in
// report.stale, so re-deriving it from the states here would badge work the run then skips.
export function staleIds(
  report: UiUpdateReport | null | undefined,
  domain: UpdateDomain,
): ReadonlySet<string> {
  return new Set(report?.stale[domain] ?? []);
}

export function staleCount(
  report: UiUpdateReport | null | undefined,
  domain: UpdateDomain,
): number {
  return report?.stale[domain].length ?? 0;
}

export function staleTotal(report: UiUpdateReport | null | undefined): number {
  if (!report) return 0;
  return Object.values(report.stale).reduce((total, ids) => total + ids.length, 0);
}

export function needsUpdatingTotal(snapshot: UiSnapshot | null | undefined): number {
  return staleTotal(snapshot?.updateReport) + (snapshot?.accountSkillsBehind.length ?? 0);
}
