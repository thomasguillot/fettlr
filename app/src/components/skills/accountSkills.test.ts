import { describe, expect, it } from "vitest";

import type { UiSkill, UiSnapshot } from "../../bindings";
import {
  CLAUDE_SKILLS_SETTINGS_URL,
  accountBehindFor,
  describeAccountBehind,
  isAccountSkill,
  packageForUpload,
} from "./accountSkills";

const t = (key: string, options?: Record<string, unknown>) =>
  options ? `${key}:${JSON.stringify(options)}` : key;

function skillWith(provenance: UiSkill["provenance"]): UiSkill {
  return { provenance } as UiSkill;
}

describe("describeAccountBehind", () => {
  it("names the local version when there is one", () => {
    expect(describeAccountBehind({ accountId: "a", localId: "l", localVersion: "4.7.1" }, t)).toBe(
      'skills.account.behindVersion:{"version":"4.7.1"}',
    );
  });

  it("falls back to the plain label", () => {
    expect(describeAccountBehind({ accountId: "a", localId: "l", localVersion: null }, t)).toBe(
      "skills.account.behind",
    );
  });
});

describe("accountBehindFor", () => {
  const entry = { accountId: "account:one", localId: "local:one", localVersion: null };
  const snapshot = { accountSkillsBehind: [entry] } as unknown as UiSnapshot;

  it("finds an entry by account id", () => {
    expect(accountBehindFor(snapshot, "account:one")).toEqual(entry);
  });

  it("returns undefined for an unknown id or a missing snapshot", () => {
    expect(accountBehindFor(snapshot, "other")).toBeUndefined();
    expect(accountBehindFor(null, "account:one")).toBeUndefined();
  });
});

describe("isAccountSkill", () => {
  it("is true only for account provenance", () => {
    expect(isAccountSkill(skillWith({ kind: "account", updatedAt: "2026-10-24T12:57:30Z" }))).toBe(true);
    expect(isAccountSkill(skillWith({ kind: "local" }))).toBe(false);
    expect(isAccountSkill(skillWith({ kind: "gitClone" } as UiSkill["provenance"]))).toBe(false);
    expect(isAccountSkill(skillWith({ kind: "skillsCli" } as UiSkill["provenance"]))).toBe(false);
    expect(isAccountSkill(skillWith({ kind: "plugin" } as UiSkill["provenance"]))).toBe(false);
  });
});

describe("packageForUpload", () => {
  const id = "plugin:impeccable@impeccable/impeccable";

  function setup(packageResult: { status: "ok"; data: string } | { status: "error" }) {
    const calls: unknown[][] = [];
    const api = {
      packageSkill: async (skillId: string) => {
        calls.push(["packageSkill", skillId]);
        return packageResult;
      },
      revealInFinder: async (path: string) => {
        calls.push(["revealInFinder", path]);
        return { status: "ok", data: null };
      },
      openUrl: async (url: string) => {
        calls.push(["openUrl", url]);
        return { status: "ok", data: null };
      },
    };
    const dispatch = async (command: Promise<{ status: string; data?: unknown }>) => {
      const result = await command;
      return result.status === "ok" ? result.data : undefined;
    };
    return { calls, api, dispatch };
  }

  it("packages, reveals the ZIP, then opens the settings page", async () => {
    const { calls, api, dispatch } = setup({ status: "ok", data: "/zips/impeccable.zip" });
    const done = await packageForUpload(dispatch as never, id, api as never);
    expect(done).toBe(true);
    expect(calls).toEqual([
      ["packageSkill", id],
      ["revealInFinder", "/zips/impeccable.zip"],
      ["openUrl", CLAUDE_SKILLS_SETTINGS_URL],
    ]);
    expect(CLAUDE_SKILLS_SETTINGS_URL).toBe("https://claude.ai/customize/skills");
  });

  it("stops when packaging fails", async () => {
    const { calls, api, dispatch } = setup({ status: "error" });
    const done = await packageForUpload(dispatch as never, id, api as never);
    expect(done).toBe(false);
    expect(calls).toEqual([["packageSkill", id]]);
  });
});
