import { Badge, Button } from "@fettlr/design-system";
import {
  RiEyeLine,
  RiEyeOffLine,
  RiFileTextLine,
  RiFolderOpenLine,
  RiRobot2Line,
  RiDeleteBinLine,
} from "@remixicon/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "../ConfirmDialog";
import { DetailEmpty, DetailHeader, DetailRow, codeChip, formatTimestamp } from "../screen";

import { commands, type UiParkedSubagent, type UiProject, type UiSubagent } from "../../bindings";
import { useFettlr } from "../../store";
import { ModelBadge, ProvenanceBadge, ScopeBadge } from "../badges";
import { parkedScopeLabel } from "../projects";
import type { SubagentSelection } from "../parkable/selection";

export function SubagentDetail({
  selection,
  onRemoved,
}: {
  selection: SubagentSelection | null;
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);

  const subagent =
    selection?.kind === "subagent"
      ? (snapshot?.subagents.find((candidate) => candidate.id === selection.id) ?? null)
      : null;
  const parked =
    selection?.kind === "parked"
      ? (snapshot?.parkedSubagents.find((candidate) => candidate.id === selection.id) ?? null)
      : null;

  if (subagent) return <ActiveSubagentDetail subagent={subagent} onRemoved={onRemoved} />;
  if (parked)
    return (
      <ParkedSubagentDetail
        parked={parked}
        projects={snapshot?.projects ?? []}
        onRemoved={onRemoved}
      />
    );

  return <DetailEmpty icon={RiRobot2Line} message={t("subagents.detail.empty")} />;
}

function ActiveSubagentDetail({ subagent, onRemoved }: { subagent: UiSubagent; onRemoved: () => void }) {
  const { t } = useTranslation();
  const dispatch = useFettlr((state) => state.dispatch);
  const select = useFettlr((state) => state.select);
  const busy = useFettlr((state) => state.busy);
  const [confirmOpen, setConfirmOpen] = useState(false);

  const isPlugin = subagent.provenance.kind === "plugin";
  const pluginId = subagent.provenance.kind === "plugin" ? subagent.provenance.pluginId : null;

  return (
    <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
      <DetailHeader title={subagent.name} />

      {isPlugin ? (
        <div className="flex items-center justify-between gap-3 rounded-md border border-border bg-bg-subtle p-3">
          <p className="text-sm text-text-muted">
            {t("subagents.detail.pluginManaged", { pluginId })}
          </p>
          <Button type="button" variant="secondary" size="sm" onClick={() => select("plugins")}>
            {t("subagents.detail.jumpToPlugin")}
          </Button>
        </div>
      ) : null}

      <dl className="flex flex-col gap-4">
        <DetailRow label={t("subagents.detail.summary")}>
          {subagent.summary ?? <span className="text-text-muted">{t("subagents.detail.noSummary")}</span>}
        </DetailRow>
        <DetailRow label={t("subagents.detail.scope")}>
          <ScopeBadge scope={subagent.scope} />
        </DetailRow>
        <DetailRow label={t("subagents.detail.provenance")}>
          <ProvenanceBadge provenance={subagent.provenance} />
        </DetailRow>
        <DetailRow label={t("subagents.detail.model")}>
          <ModelBadge model={subagent.model} />
        </DetailRow>
        <DetailRow label={t("subagents.detail.tools")}>
          {subagent.tools.length > 0 ? (
            <div className="flex flex-wrap gap-1">
              {subagent.tools.map((tool) => (
                <Badge key={tool} variant="outline">
                  {tool}
                </Badge>
              ))}
            </div>
          ) : (
            <span className="text-text-muted">{t("subagents.detail.allTools")}</span>
          )}
        </DetailRow>
        {subagent.effort ? (
          <DetailRow label={t("subagents.detail.effort")}>{subagent.effort}</DetailRow>
        ) : null}
        {subagent.permissionMode ? (
          <DetailRow label={t("subagents.detail.permissionMode")}>{subagent.permissionMode}</DetailRow>
        ) : null}
        {subagent.maxTurns !== null ? (
          <DetailRow label={t("subagents.detail.maxTurns")}>
            <span className="tabular-nums">{subagent.maxTurns}</span>
          </DetailRow>
        ) : null}
        {subagent.memory ? (
          <DetailRow label={t("subagents.detail.memory")}>{subagent.memory}</DetailRow>
        ) : null}
        <DetailRow label={t("subagents.detail.path")}>
          <code className={codeChip}>{subagent.path}</code>
        </DetailRow>
      </dl>

      <div className="mt-auto grid grid-cols-2 gap-2">
        <Button
          type="button"
          variant="secondary"
          onClick={() => void dispatch(commands.revealInFinder(subagent.path))}
        >
          <RiFolderOpenLine className="size-4" aria-hidden="true" />
          {t("subagents.detail.revealInFinder")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          onClick={() => void dispatch(commands.openPath(subagent.path))}
        >
          <RiFileTextLine className="size-4" aria-hidden="true" />
          {t("subagents.detail.openFile")}
        </Button>
        {!isPlugin ? (
          <Button
            type="button"
            variant="secondary"
            disabled={busy}
            onClick={() =>
              void dispatch(commands.parkSubagent(subagent.id)).then((result) => {
                if (result) onRemoved();
              })
            }
          >
            <RiEyeOffLine className="size-4" aria-hidden="true" />
            {t("subagents.detail.disable")}
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
        title={t("subagents.detail.deleteTitle")}
        description={t("subagents.detail.deleteDescription", { name: subagent.name })}
        confirmLabel={t("common.delete")}
        onConfirm={() => {
          void dispatch(commands.deleteSubagent(subagent.id)).then((result) => {
            if (result) onRemoved();
          });
        }}
      />
    </div>
  );
}

function ParkedSubagentDetail({
  parked,
  projects,
  onRemoved,
}: {
  parked: UiParkedSubagent;
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
          {t("subagents.parkedBadge")}
        </Badge>
      </div>

      <dl className="flex flex-col gap-4">
        <DetailRow label={t("subagents.detail.scope")}>{parkedScopeLabel(parked, projects, t)}</DetailRow>
        <DetailRow label={t("subagents.detail.parkedAt")}>{formatTimestamp(parked.parkedAt)}</DetailRow>
        <DetailRow label={t("subagents.detail.originalPath")}>
          <code className={codeChip}>{parked.originalPath}</code>
        </DetailRow>
        <DetailRow label={t("subagents.detail.path")}>
          <code className={codeChip}>{parked.parkedPath}</code>
        </DetailRow>
      </dl>

      <div className="mt-auto grid grid-cols-2 gap-2">
        <Button
          type="button"
          variant="secondary"
          onClick={() => void dispatch(commands.revealInFinder(parked.parkedPath))}
        >
          <RiFolderOpenLine className="size-4" aria-hidden="true" />
          {t("subagents.detail.revealInFinder")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          onClick={() => void dispatch(commands.openPath(parked.parkedPath))}
        >
          <RiFileTextLine className="size-4" aria-hidden="true" />
          {t("subagents.detail.openFile")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={busy}
          onClick={() =>
            void dispatch(commands.restoreSubagent(parked.id)).then((result) => {
              if (result) onRemoved();
            })
          }
        >
          <RiEyeLine className="size-4" aria-hidden="true" />
          {t("subagents.detail.enable")}
        </Button>
        <Button type="button" variant="dangerGhost" disabled={busy} onClick={() => setConfirmOpen(true)}>
          <RiDeleteBinLine className="size-4" aria-hidden="true" />
          {t("common.delete")}
        </Button>
      </div>

      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t("subagents.detail.deleteTitle")}
        description={t("subagents.detail.deleteDescription", { name: parked.name })}
        confirmLabel={t("common.delete")}
        onConfirm={() => {
          void dispatch(commands.deleteParkedSubagent(parked.id)).then((result) => {
            if (result) onRemoved();
          });
        }}
      />
    </div>
  );
}
