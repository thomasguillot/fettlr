import { type UiUpdateFailure } from "./bindings";

export function failureKey(kind: UiUpdateFailure["kind"]): string {
  switch (kind) {
    case "marketplace":
      return "updateAll.failureMarketplace";
    case "plugin":
      return "updateAll.failurePlugin";
    case "skill":
      return "updateAll.failureSkill";
  }
}
