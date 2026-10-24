import { Badge, Button } from "@fettlr/design-system";
import { RiFileTextLine, RiFileZipLine, RiFolderOpenLine } from "@remixicon/react";
import { useTranslation } from "react-i18next";

import { commands, type UiSkill } from "../../bindings";
import { useFettlr } from "../../store";
import { ProvenanceBadge, StaleBadge } from "../badges";
import { itemKey } from "../parkable/selection";
import { DetailHeader, DetailRow, UsageRow, codeChip, formatTimestamp } from "../screen";
import {
  CLAUDE_SKILLS_SETTINGS_URL,
  accountBehindFor,
  describeAccountBehind,
  packageForUpload,
} from "./accountSkills";

export function AccountSkillDetail({ skill }: { skill: UiSkill }) {
  const { t } = useTranslation();
  const dispatch = useFettlr((state) => state.dispatch);
  const navigateToItem = useFettlr((state) => state.navigateToItem);
  const busy = useFettlr((state) => state.busy);
  const behind = useFettlr((state) => accountBehindFor(state.snapshot, skill.id));
  const localPath = useFettlr((state) =>
    behind ? state.snapshot?.skills.find((candidate) => candidate.id === behind.localId)?.path : undefined,
  );
  const updatedAt = skill.provenance.kind === "account" ? skill.provenance.updatedAt : null;

  return (
    <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
      <DetailHeader title={skill.title ?? skill.name} />

      {skill.title && skill.title !== skill.name ? (
        <p className="font-mono text-sm text-text-muted">{skill.name}</p>
      ) : null}

      <div className="flex items-center justify-between gap-3 rounded-md border border-border bg-bg-subtle p-3">
        <p className="text-sm text-text-muted">{t("skills.detail.accountManaged")}</p>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          onClick={() => void dispatch(commands.openUrl(CLAUDE_SKILLS_SETTINGS_URL))}
        >
          {t("skills.detail.openClaudeSettings")}
        </Button>
      </div>

      <dl className="flex flex-col gap-4">
        <DetailRow label={t("skills.detail.summary")}>
          {skill.summary ?? <span className="text-text-muted">{t("skills.detail.noSummary")}</span>}
        </DetailRow>
        <DetailRow label={t("skills.detail.provenance")}>
          <ProvenanceBadge provenance={skill.provenance} />
        </DetailRow>
        <UsageRow usage={skill.usage} />
        {updatedAt ? (
          <DetailRow label={t("skills.detail.accountUpdatedAt")}>{formatTimestamp(updatedAt)}</DetailRow>
        ) : null}
        <DetailRow label={t("skills.detail.updateStatus")}>
          {behind ? (
            <StaleBadge>{describeAccountBehind(behind, t)}</StaleBadge>
          ) : (
            <Badge tone="neutral" variant="soft">
              {t("skills.account.noNewerCopy")}
            </Badge>
          )}
        </DetailRow>
        {behind ? (
          <DetailRow label={t("skills.detail.newerLocalCopy")}>
            <div className="flex items-center justify-between gap-3">
              {localPath ? <code className={codeChip}>{localPath}</code> : <span />}
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => navigateToItem("skills", itemKey("skill", behind.localId))}
              >
                {t("skills.detail.showLocalCopy")}
              </Button>
            </div>
          </DetailRow>
        ) : null}
        <DetailRow label={t("skills.detail.path")}>
          <code className={codeChip}>{skill.path}</code>
        </DetailRow>
      </dl>

      {behind ? <p className="text-sm text-text-muted">{t("skills.account.packageHint")}</p> : null}

      <div className="mt-auto grid grid-cols-2 gap-2">
        <Button
          type="button"
          variant="secondary"
          onClick={() => void dispatch(commands.revealInFinder(skill.path))}
        >
          <RiFolderOpenLine className="size-4" aria-hidden="true" />
          {t("skills.detail.revealInFinder")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          onClick={() =>
            void dispatch(commands.openPath(`${skill.path.replace(/\/+$/, "")}/SKILL.md`))
          }
        >
          <RiFileTextLine className="size-4" aria-hidden="true" />
          {t("skills.detail.openSkillMd")}
        </Button>
        {behind ? (
          <Button
            type="button"
            disabled={busy}
            onClick={() => void packageForUpload(dispatch, behind.localId)}
          >
            <RiFileZipLine className="size-4" aria-hidden="true" />
            {t("skills.account.package")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}
