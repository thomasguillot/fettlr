import { describe, expect, it } from "vitest";

import { repinnedArgs } from "./mcpConfig";

describe("repinnedArgs", () => {
  it("rewrites the npm version", () => {
    expect(repinnedArgs("npx", ["-y", "@scope/pkg@1.2.3"], "2.0.0")).toEqual(["-y", "@scope/pkg@2.0.0"]);
  });

  it("keeps the separator the package used", () => {
    expect(repinnedArgs("uvx", ["pkg==1.0"], "1.1")).toEqual(["pkg==1.1"]);
    expect(repinnedArgs("uvx", ["pkg@1.0"], "1.1")).toEqual(["pkg@1.1"]);
  });

  it("leaves every other argument alone", () => {
    expect(repinnedArgs("/opt/homebrew/bin/npx", ["-p", "pkg@1.0.0", "binary", "--flag"], "1.1.0")).toEqual([
      "-p",
      "pkg@1.1.0",
      "binary",
      "--flag",
    ]);
  });

  it("refuses anything that is not pinned", () => {
    expect(repinnedArgs("npx", ["-y", "pkg"], "1.0.0")).toBeNull();
    expect(repinnedArgs("npx", ["pkg@latest"], "1.0.0")).toBeNull();
    expect(repinnedArgs("node", ["server.js"], "1.0.0")).toBeNull();
    expect(repinnedArgs(null, ["pkg@1.0.0"], "1.1.0")).toBeNull();
    expect(repinnedArgs("npx", [], "1.1.0")).toBeNull();
  });
});
