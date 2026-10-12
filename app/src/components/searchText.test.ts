import { describe, expect, it } from "vitest";

import type { UiMarketplace, UiMcpServer, UiSkill } from "../bindings";
import { marketplaceSearchValue, matchesText, mcpSearchValue, skillSearchValue } from "./searchText";

const skill: UiSkill = {
  id: "user:pdf-tools",
  name: "pdf-tools",
  title: "PDF Tools",
  summary: "Split and merge PDFs",
  scope: { kind: "user" },
  provenance: { kind: "local" },
  path: "/tmp/pdf-tools",
  usage: null,
};

const server: UiMcpServer = {
  id: "project:/Users/me/site:context-server",
  name: "context-server",
  origin: { kind: "projectFile", project: "/Users/me/site" },
  transport: "stdio",
  isEnabled: true,
  config: {
    kind: "stdio",
    command: "npx",
    args: ["-y", "context-server@2.1.0"],
    maskedArgs: [],
    url: null,
    urlMasked: false,
    env: {},
    envKeys: [],
    headers: {},
    headerKeys: [],
  },
};

const marketplace: UiMarketplace = {
  name: "accesslint",
  source: "github",
  repo: "accesslint/claude-marketplace",
  url: null,
  installLocation: "/Users/me/.claude/plugins/marketplaces/accesslint",
  displaySource: "accesslint/claude-marketplace",
};

describe("matchesText", () => {
  it("matches every value when the query is blank", () => {
    expect(matchesText("anything", "")).toBe(true);
    expect(matchesText("anything", "   ")).toBe(true);
  });

  it("is a case-insensitive substring test that ignores surrounding spaces", () => {
    expect(matchesText("PDF Tools", "pdf")).toBe(true);
    expect(matchesText("pdf tools", "  TOOLS ")).toBe(true);
    expect(matchesText("pdf tools", "pdx")).toBe(false);
  });

  it("never matches letters spread across the value", () => {
    expect(matchesText("figure junk", "fgjk")).toBe(false);
  });
});

describe("search values", () => {
  it("lets a skill be found by title, name, or summary", () => {
    const value = skillSearchValue(skill);
    expect(matchesText(value, "PDF Tools")).toBe(true);
    expect(matchesText(value, "pdf-tools")).toBe(true);
    expect(matchesText(value, "merge")).toBe(true);
    expect(matchesText(value, "spreadsheet")).toBe(false);
  });

  it("lets a project server be found by its project and transport", () => {
    const value = mcpSearchValue(server);
    expect(matchesText(value, "Users/me/site")).toBe(true);
    expect(matchesText(value, "stdio")).toBe(true);
    expect(matchesText(value, "context")).toBe(true);
  });

  it("lets a marketplace be found by what its Source column shows", () => {
    const value = marketplaceSearchValue(marketplace);
    expect(matchesText(value, "claude-marketplace")).toBe(true);
    expect(matchesText(value, "github")).toBe(true);
    expect(matchesText(value, "gitlab")).toBe(false);
  });
});
