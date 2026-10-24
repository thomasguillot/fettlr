import { commands, type UiAccountSkillBehind, type UiSkill, type UiSnapshot } from "../../bindings";
import type { CommandResult } from "../../store";

type Translate = (key: string, options?: Record<string, unknown>) => string;

export const CLAUDE_SKILLS_SETTINGS_URL = "https://claude.ai/customize/skills";

export function isAccountSkill(skill: UiSkill): boolean {
  return skill.provenance.kind === "account";
}

export function accountBehindFor(
  snapshot: UiSnapshot | null | undefined,
  accountId: string,
): UiAccountSkillBehind | undefined {
  return snapshot?.accountSkillsBehind.find((entry) => entry.accountId === accountId);
}

export function describeAccountBehind(entry: UiAccountSkillBehind, t: Translate): string {
  return entry.localVersion
    ? t("skills.account.behindVersion", { version: entry.localVersion })
    : t("skills.account.behind");
}

export async function packageForUpload(
  dispatch: <T>(command: Promise<CommandResult<T>>) => Promise<T | undefined>,
  localId: string,
  api: Pick<typeof commands, "packageSkill" | "revealInFinder" | "openUrl"> = commands,
): Promise<boolean> {
  const zipPath = await dispatch(api.packageSkill(localId));
  if (zipPath === undefined) return false;
  await dispatch(api.revealInFinder(zipPath));
  await dispatch(api.openUrl(CLAUDE_SKILLS_SETTINGS_URL));
  return true;
}
