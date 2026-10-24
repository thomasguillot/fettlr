import { describe, expect, it } from "vitest";

import { describeUpdateState } from "./updateState";

const t = (key: string) => key;

describe("describeUpdateState", () => {
  it("words a source change as an available update", () => {
    expect(describeUpdateState({ kind: "sourceChanged" }, t)).toBe("common.updateAvailable");
  });

  it("keeps the existing kinds", () => {
    expect(describeUpdateState({ kind: "behind", commits: 2 }, t)).toBe("common.commitsBehind");
    expect(describeUpdateState({ kind: "updateAvailable", installed: "1", latest: "2" }, t)).toBe(
      "common.versionChange",
    );
    expect(describeUpdateState({ kind: "upToDate" }, t)).toBeNull();
  });
});
