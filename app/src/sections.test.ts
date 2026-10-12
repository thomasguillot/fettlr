import { describe, expect, it } from "vitest";

import type { UiSnapshot, UiUpdateReport, UiUpdateState } from "./bindings";
import { sectionCount, sections, staleCount, staleIds, staleTotal } from "./sections";

function report(overrides: Partial<UiUpdateReport>): UiUpdateReport {
  return {
    marketplaces: {},
    plugins: {},
    skills: {},
    mcpServers: {},
    stale: { marketplaces: [], plugins: [], skills: [], mcpServers: [] },
    checkedAt: "2026-08-24T10:00:00Z",
    ...overrides,
  };
}

const behind: UiUpdateState = { kind: "behind", commits: 3 };
const knownNewer: UiUpdateState = { kind: "updateAvailable", installed: "1.0.0", latest: "2.0.0" };
const versionUnknown: UiUpdateState = {
  kind: "updateAvailable",
  installed: "1.0.0",
  latest: "unknown",
};

describe("staleness comes from the core, not from the states", () => {
  it("counts what the report calls stale", () => {
    const snapshot = report({
      plugins: { "widgets@acme#user": knownNewer, "gadgets@acme#user": { kind: "upToDate" } },
      stale: {
        marketplaces: [],
        plugins: ["widgets@acme#user"],
        skills: [],
        mcpServers: [],
      },
    });

    expect(staleCount(snapshot, "plugins")).toBe(1);
    expect(staleIds(snapshot, "plugins").has("widgets@acme#user")).toBe(true);
    expect(staleIds(snapshot, "plugins").has("gadgets@acme#user")).toBe(false);
  });

  // The T8 regression: an update run skips a catalogue entry with no version of its own, so
  // counting it here would badge work that never happens and then report success over an empty run.
  it("never counts a state the core left out of its stale list", () => {
    const snapshot = report({
      plugins: { "widgets@acme#user": versionUnknown },
      marketplaces: { acme: behind },
      stale: { marketplaces: ["acme"], plugins: [], skills: [], mcpServers: [] },
    });

    expect(staleCount(snapshot, "plugins")).toBe(0);
    expect(staleIds(snapshot, "plugins").size).toBe(0);
    expect(staleCount(snapshot, "marketplaces")).toBe(1);
  });

  it("reads zero out of a snapshot that has never been checked", () => {
    expect(staleCount(null, "skills")).toBe(0);
    expect(staleIds(undefined, "skills").size).toBe(0);
  });

  it("totals every list the core calls stale, the number the dock badge shows", () => {
    const snapshot = report({
      stale: { marketplaces: ["acme"], plugins: ["widgets@acme#user"], skills: [], mcpServers: ["user:fetch"] },
    });

    expect(staleTotal(snapshot)).toBe(3);
    expect(staleTotal(null)).toBe(0);
  });
});

function snapshotWith(overrides: Partial<UiSnapshot>): UiSnapshot {
  return {
    cliAvailable: true,
    skills: [],
    parkedSkills: [],
    subagents: [],
    parkedSubagents: [],
    plugins: [],
    availablePlugins: [],
    marketplaces: [],
    mcpServers: [],
    projects: [],
    updateReport: null,
    warnings: [],
    updateFailures: [],
    lastRefreshed: null,
    connectors: [],
    connectorsCheckedAt: null,
    ...overrides,
  };
}

describe("section counts", () => {
  const subagents = sections.find((section) => section.id === "subagents")!;
  const updates = sections.find((section) => section.id === "updates")!;

  it("counts parked subagents with the active ones", () => {
    const snapshot = snapshotWith({
      subagents: [{ id: "user/a" } as UiSnapshot["subagents"][number]],
      parkedSubagents: [{ id: "/p/b.md" } as UiSnapshot["parkedSubagents"][number]],
    });
    expect(sectionCount(snapshot, subagents)).toBe(2);
  });

  it("reads zero for a section without a count and for no snapshot", () => {
    expect(sectionCount(snapshotWith({}), updates)).toBe(0);
    expect(sectionCount(null, subagents)).toBe(0);
  });

  it("keeps subagents out of the update domains", () => {
    expect(subagents.domain).toBeNull();
  });

  it("counts connectors only after a check", () => {
    const connectors = sections.find((section) => section.id === "connectors")!;
    const unchecked = snapshotWith({
      connectors: [{ state: "connected" } as UiSnapshot["connectors"][number]],
      connectorsCheckedAt: null,
    });
    const checked = snapshotWith({
      connectors: [
        { state: "connected" } as UiSnapshot["connectors"][number],
        { state: "failed" } as UiSnapshot["connectors"][number],
      ],
      connectorsCheckedAt: "2026-08-30T10:00:00Z",
    });
    expect(sectionCount(unchecked, connectors)).toBe(0);
    expect(sectionCount(checked, connectors)).toBe(1);
  });
});
