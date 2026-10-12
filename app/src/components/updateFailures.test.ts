import { describe, expect, it } from "vitest";

import en from "../locales/en/common.json";
import { failureKey } from "./UpdateAllSheet";

function lookup(key: string): unknown {
  return key.split(".").reduce<unknown>((node, part) => {
    if (typeof node !== "object" || node === null) return undefined;
    return (node as Record<string, unknown>)[part];
  }, en);
}

describe("an update run's failures are worded here, not parsed out of core's text", () => {
  it("gives every failure kind a key of its own", () => {
    expect(failureKey("marketplace")).toBe("updateAll.failureMarketplace");
    expect(failureKey("plugin")).toBe("updateAll.failurePlugin");
    expect(failureKey("skill")).toBe("updateAll.failureSkill");
  });

  it("names a string that exists and carries both interpolations", () => {
    for (const kind of ["marketplace", "plugin", "skill"] as const) {
      const value = lookup(failureKey(kind));
      expect(typeof value).toBe("string");
      expect(value as string).toContain("{{name}}");
      expect(value as string).toContain("{{error}}");
    }
  });
});
