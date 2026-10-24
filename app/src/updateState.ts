import type { UiUpdateState } from "./bindings";

type Translate = (key: string, options?: Record<string, unknown>) => string;

// The one place that words a pending update. Each domain's badge still decides which state kinds it
// can receive, but none of them has to know how to phrase one, so a domain that starts reporting a
// kind it never used to report reads correctly everywhere at once.
export function describeUpdateState(
  state: UiUpdateState | undefined,
  t: Translate,
): string | null {
  if (state?.kind === "behind") {
    return t("common.commitsBehind", { count: state.commits });
  }
  if (state?.kind === "updateAvailable") {
    return t("common.versionChange", { installed: state.installed, latest: state.latest });
  }
  if (state?.kind === "sourceChanged") {
    return t("common.updateAvailable");
  }
  return null;
}
