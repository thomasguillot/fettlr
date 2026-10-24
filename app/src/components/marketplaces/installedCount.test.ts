import { describe, expect, it } from "vitest";

import type { UiPlugin } from "../../bindings";
import { installedFromMarketplace } from "./installedCount";

function plugin(overrides: Partial<UiPlugin>): UiPlugin {
  return {
    id: "p",
    pluginId: "p@acme",
    name: "p",
    description: null,
    marketplaceName: "acme",
    version: "1.0.0",
    scope: { kind: "user" },
    enabled: true,
    installPath: "/tmp/p",
    installedAt: null,
    lastUpdated: null,
    isAvailableOnly: false,
    mcpServerNames: [],
    usage: null,
    ...overrides,
  };
}

describe("installedFromMarketplace", () => {
  it("counts installed plugins of the marketplace across scopes", () => {
    const list = [
      plugin({ id: "a" }),
      plugin({ id: "b", scope: { kind: "project", path: "/work" } }),
      plugin({ id: "c", marketplaceName: "other" }),
    ];
    expect(installedFromMarketplace(list, "acme")).toBe(2);
  });

  it("ignores available-only entries", () => {
    expect(installedFromMarketplace([plugin({ isAvailableOnly: true })], "acme")).toBe(0);
  });

  it("is zero for a marketplace with no plugins", () => {
    expect(installedFromMarketplace([plugin({})], "impeccable")).toBe(0);
  });
});
