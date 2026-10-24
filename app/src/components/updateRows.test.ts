import { describe, expect, it } from "vitest";

import { type UpdateRow, updateAllCount } from "./updateRows";

function row(overrides: Partial<UpdateRow>): UpdateRow {
  return {
    id: "x",
    kind: "plugin",
    name: "x",
    note: null,
    detail: "",
    action: () => undefined,
    actionKey: "common.update",
    inUpdateAll: true,
    ...overrides,
  };
}

describe("updateAllCount", () => {
  it("account_rows_count_toward_the_total_but_not_update_all", () => {
    const rows = [
      row({ kind: "plugin" }),
      row({ kind: "mcpServer", inUpdateAll: false }),
      row({ kind: "accountSkill", inUpdateAll: false }),
      row({ kind: "plugin", action: null }),
    ];
    expect(updateAllCount(rows)).toBe(1);
  });
});
