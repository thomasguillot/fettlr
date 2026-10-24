import { Badge, Button } from "@fettlr/design-system";
import {
  RiArrowUpCircleLine,
  RiEyeLine,
  RiEyeOffLine,
  RiFileTextLine,
  RiFolderOpenLine,
  RiLoader4Line,
  RiSparklingLine,
  RiDeleteBinLine,
} from "@remixicon/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "../ConfirmDialog";
import { DetailEmpty, DetailHeader, DetailRow, UsageRow, codeChip, formatTimestamp } from "../screen";

import { commands, type UiParkedSkill, type UiProject, type UiSkill } from "../../bindings";
import { staleIds } from "../../sections";
import { useFettlr } from "../../store";
import { useUpdateAction } from "../../useUpdateAction";
import { ProvenanceBadge, ScopeBadge, UpdateStatusBadge } from "../badges";
import { parkedScopeLabel } from "../projects";
import type { SkillSelection } from "../parkable/selection";

function skillMarkdownPath(folderPath: string): string {
  return `${folderPath.replace(/\/+$/, "")}/SKILL.md`;
}

export function SkillDetail({
  selection,
  onRemoved,
}: {
  selection: SkillSelection | null;
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);

  const skill =
    selection?.kind === "skill"
      ? (snapshot?.skills.find((candidate) => candidate.id === selection.id) ?? null)
      : null;
  const parked =
    selection?.kind === "parked"
      ? (snapshot?.parkedSkills.find((candidate) => candidate.id === selection.id) ?? null)
      : null;

  if (skill) return <ActiveSkillDetail key={skill.id} skill={skill} onRemoved={onRemoved} />;
  if (parked)
    return (
      <ParkedSkillDetail
        key={parked.id}
        parked={parked}
        projects={snapshot?.projects ?? []}
        onRemoved={onRemoved}
      />
    );

  return (
    <DetailEmpty icon={RiSparklingLine} message={t("skills.detail.empty")} />
  );
}

function ActiveSkillDetail({ skill, onRemoved }: { skill: UiSkill; onRemoved: () => void }) {
  const { t } = useTranslation();
  const dispatch = useFettlr((state) => state.dispatch);
  const { run, isRunning } = useUpdateAction();
  const select = useFettlr((state) => state.select);
  const busy = useFettlr((state) => state.busy);
  const updateReport = useFettlr((state) => state.snapshot?.updateReport ?? null);
  const [confirmOpen, setConfirmOpen] = useState(false);

  const isPlugin = skill.provenance.kind === "plugin";
  const isGitClone = skill.provenance.kind === "gitClone";
  const isSkillsCli = skill.provenance.kind === "skillsCli";
  const isUpdatable = isGitClone || isSkillsCli;
  const pluginId = skill.provenance.kind === "plugin" ? skill.provenance.pluginId : null;
  const updateState = updateReport?.skills[skill.id];
  const stale = staleIds(updateReport, "skills").has(skill.id);

  const pulling = isRunning(skill.id);
  const handlePull = () => run(skill.id, () => dispatch(commands.pullSkill(skill.id)));

  return (
    <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
      <DetailHeader title={skill.title ?? skill.name} />

      {skill.title && skill.title !== skill.name ? (
        <p className="font-mono text-sm text-text-muted">{skill.name}</p>
      ) : null}

      {isPlugin ? (
        <div className="flex items-center justify-between gap-3 rounded-md border border-border bg-bg-subtle p-3">
          <p className="text-sm text-text-muted">
            {t("skills.detail.pluginManaged", { pluginId })}
          </p>
          <Button type="button" variant="secondary" size="sm" onClick={() => select("plugins")}>
            {t("skills.detail.jumpToPlugin")}
          </Button>
        </div>
      ) : null}

      <dl className="flex flex-col gap-4">
        <DetailRow label={t("skills.detail.summary")}>
          {skill.summary ?? <span className="text-text-muted">{t("skills.detail.noSummary")}</span>}
        </DetailRow>
        <DetailRow label={t("skills.detail.scope")}>
          <ScopeBadge scope={skill.scope} />
        </DetailRow>
        <DetailRow label={t("skills.detail.provenance")}>
          <ProvenanceBadge provenance={skill.provenance} />
        </DetailRow>
        {skill.provenance.kind === "skillsCli" ? (
          <DetailRow label={t("skills.detail.source")}>
            <code className={codeChip}>{skill.provenance.source}</code>
          </DetailRow>
        ) : null}
        <UsageRow usage={skill.usage} />
        {isUpdatable ? (
          <DetailRow label={t("skills.detail.updateStatus")}>
            <UpdateStatusBadge state={updateState} pending={isGitClone ? "behind" : "sourceChanged"} />
          </DetailRow>
        ) : null}
        <DetailRow label={t("skills.detail.path")}>
          <code className={codeChip}>
            {skill.path}
          </code>
        </DetailRow>
      </dl>

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
          onClick={() => void dispatch(commands.openPath(skillMarkdownPath(skill.path)))}
        >
          <RiFileTextLine className="size-4" aria-hidden="true" />
          {t("skills.detail.openSkillMd")}
        </Button>
        {isUpdatable ? (
          <Button
            type="button"
            variant="secondary"
            disabled={busy || pulling || !stale}
            onClick={() => void handlePull()}
          >
            {pulling ? (
              <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
            ) : (
              <RiArrowUpCircleLine className="size-4" aria-hidden="true" />
            )}
            {isGitClone ? t("skills.detail.pull") : t("common.update")}
          </Button>
        ) : null}
        {!isPlugin ? (
          <Button
            type="button"
            variant="secondary"
            disabled={busy}
            onClick={() =>
              void dispatch(commands.parkSkill(skill.id)).then((result) => {
                if (result) onRemoved();
              })
            }
          >
            <RiEyeOffLine className="size-4" aria-hidden="true" />
            {t("skills.detail.disable")}
          </Button>
        ) : null}
        {!isPlugin ? (
          <Button type="button" variant="dangerGhost" disabled={busy} onClick={() => setConfirmOpen(true)}>
            <RiDeleteBinLine className="size-4" aria-hidden="true" />
            {t("common.delete")}
          </Button>
        ) : null}
      </div>

      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t("skills.detail.deleteTitle")}
        description={t("skills.detail.deleteDescription", { name: skill.title ?? skill.name })}
        confirmLabel={t("common.delete")}
        onConfirm={() => {
          void dispatch(commands.deleteSkill(skill.id)).then((result) => {
            if (result) onRemoved();
          });
        }}
      />
    </div>
  );
}

function ParkedSkillDetail({
  parked,
  projects,
  onRemoved,
}: {
  parked: UiParkedSkill;
  projects: UiProject[];
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const dispatch = useFettlr((state) => state.dispatch);
  const busy = useFettlr((state) => state.busy);
  const [confirmOpen, setConfirmOpen] = useState(false);

  return (
    <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
      <DetailHeader title={parked.name} />

      <div className="flex items-center gap-2">
        <Badge tone="neutral" variant="soft">
          {t("skills.parkedBadge")}
        </Badge>
      </div>

      <dl className="flex flex-col gap-4">
        <DetailRow label={t("skills.detail.scope")}>{parkedScopeLabel(parked, projects, t)}</DetailRow>
        <DetailRow label={t("skills.detail.parkedAt")}>{formatTimestamp(parked.parkedAt)}</DetailRow>
        <DetailRow label={t("skills.detail.originalPath")}>
          <code className={codeChip}>
            {parked.originalPath}
          </code>
        </DetailRow>
        <DetailRow label={t("skills.detail.path")}>
          <code className={codeChip}>
            {parked.parkedPath}
          </code>
        </DetailRow>
      </dl>

      <div className="mt-auto grid grid-cols-2 gap-2">
        <Button
          type="button"
          variant="secondary"
          onClick={() => void dispatch(commands.revealInFinder(parked.parkedPath))}
        >
          <RiFolderOpenLine className="size-4" aria-hidden="true" />
          {t("skills.detail.revealInFinder")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          onClick={() => void dispatch(commands.openPath(skillMarkdownPath(parked.parkedPath)))}
        >
          <RiFileTextLine className="size-4" aria-hidden="true" />
          {t("skills.detail.openSkillMd")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={busy}
          onClick={() =>
            void dispatch(commands.restoreSkill(parked.id)).then((result) => {
              if (result) onRemoved();
            })
          }
        >
          <RiEyeLine className="size-4" aria-hidden="true" />
          {t("skills.detail.enable")}
        </Button>
        <Button type="button" variant="dangerGhost" disabled={busy} onClick={() => setConfirmOpen(true)}>
          <RiDeleteBinLine className="size-4" aria-hidden="true" />
          {t("common.delete")}
        </Button>
      </div>

      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t("skills.detail.deleteTitle")}
        description={t("skills.detail.deleteDescription", { name: parked.name })}
        confirmLabel={t("common.delete")}
        onConfirm={() => {
          void dispatch(commands.deleteParkedSkill(parked.id)).then((result) => {
            if (result) onRemoved();
          });
        }}
      />
    </div>
  );
}
