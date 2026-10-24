import { Button } from "@fettlr/design-system";
import { RiArrowUpCircleLine, RiLoader4Line, RiStoreLine, RiDeleteBinLine } from "@remixicon/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "../ConfirmDialog";
import { DetailEmpty, DetailHeader, DetailRow, capsHeading, codeChip } from "../screen";

import { commands, type UiMarketplace } from "../../bindings";
import { staleIds } from "../../sections";
import { useFettlr } from "../../store";
import { useUpdateAction } from "../../useUpdateAction";
import { UpdateStatusBadge } from "../badges";
import { installedFromMarketplace } from "./installedCount";

export function MarketplaceDetail({
  marketplaceName,
  onRemoved,
}: {
  marketplaceName: string | null;
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const marketplace = useFettlr((state) =>
    marketplaceName
      ? (state.snapshot?.marketplaces.find((candidate) => candidate.name === marketplaceName) ?? null)
      : null,
  );

  if (!marketplace) {
    return (
      <DetailEmpty icon={RiStoreLine} message={t("marketplaces.detail.empty")} />
    );
  }

  return <ActiveMarketplaceDetail key={marketplace.name} marketplace={marketplace} onRemoved={onRemoved} />;
}

function ActiveMarketplaceDetail({
  marketplace,
  onRemoved,
}: {
  marketplace: UiMarketplace;
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);
  const dispatch = useFettlr((state) => state.dispatch);
  const busy = useFettlr((state) => state.busy);
  const { run, isRunning } = useUpdateAction();
  const [confirmOpen, setConfirmOpen] = useState(false);

  const updateReport = snapshot?.updateReport ?? null;
  const state = updateReport?.marketplaces[marketplace.name];
  const stale = staleIds(updateReport, "marketplaces").has(marketplace.name);
  const dependentPlugins = (snapshot?.plugins ?? []).filter(
    (plugin) => plugin.marketplaceName === marketplace.name,
  );
  const installedCount = installedFromMarketplace(snapshot?.plugins ?? [], marketplace.name);
  const availableCount = new Set(
    (snapshot?.availablePlugins ?? [])
      .filter((plugin) => plugin.marketplaceName === marketplace.name)
      .map((plugin) => plugin.pluginId),
  ).size;

  const updating = isRunning(marketplace.name);
  const handleUpdate = () => run(marketplace.name, () => dispatch(commands.updateMarketplace(marketplace.name)));

  const handleRemove = () => {
    void dispatch(commands.removeMarketplace(marketplace.name)).then((result) => {
      if (result) onRemoved();
    });
  };

  return (
    <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
      <DetailHeader title={marketplace.name} />

      <p className="truncate font-mono text-sm text-text-muted">{marketplace.displaySource}</p>

      <dl className="flex flex-col gap-4">
        <DetailRow label={t("marketplaces.detail.source")}>
          <code className={codeChip}>
            {marketplace.source}
          </code>
        </DetailRow>
        <DetailRow label={t("marketplaces.detail.installLocation")}>
          <code className={codeChip}>
            {marketplace.installLocation}
          </code>
        </DetailRow>
        <DetailRow label={t("marketplaces.detail.installedCount")}>{installedCount}</DetailRow>
        <DetailRow label={t("marketplaces.detail.availableCount")}>{availableCount}</DetailRow>
        <DetailRow label={t("marketplaces.detail.updateStatus")}>
          <UpdateStatusBadge state={state} pending="behind" />
        </DetailRow>
      </dl>

      <section className="flex flex-col gap-2">
        <h3 className={capsHeading}>
          {t("marketplaces.detail.pluginsHeading")}
        </h3>
        {dependentPlugins.length === 0 ? (
          <p className="text-sm text-text-muted">{t("marketplaces.detail.noPlugins")}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {dependentPlugins.map((plugin) => (
              <li key={plugin.id} className="flex items-center justify-between gap-2 text-sm text-text">
                <span className="truncate">{plugin.name}</span>
                <span className="shrink-0 text-xs text-text-muted">{plugin.version}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <div className="mt-auto grid grid-cols-2 gap-2">
        <Button
          type="button"
          variant="secondary"
          disabled={busy || updating || !stale}
          onClick={() => void handleUpdate()}
        >
          {updating ? (
            <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
          ) : (
            <RiArrowUpCircleLine className="size-4" aria-hidden="true" />
          )}
          {t("marketplaces.detail.update")}
        </Button>
        <Button type="button" variant="dangerGhost" disabled={busy} onClick={() => setConfirmOpen(true)}>
          <RiDeleteBinLine className="size-4" aria-hidden="true" />
          {t("marketplaces.detail.remove")}
        </Button>
      </div>

      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t("marketplaces.detail.removeTitle")}
        description={
          dependentPlugins.length > 0
            ? t("marketplaces.detail.removeDescriptionWithDeps", { name: marketplace.name })
            : t("marketplaces.detail.removeDescriptionNoDeps", { name: marketplace.name })
        }
        confirmLabel={t("marketplaces.detail.remove")}
        onConfirm={handleRemove}
      >
        {dependentPlugins.length > 0 ? (
          <ul className="flex flex-col gap-1 rounded-md border border-border bg-bg-subtle p-2.5 text-sm text-text">
            {dependentPlugins.map((plugin) => (
              <li key={plugin.id}>{plugin.name}</li>
            ))}
          </ul>
        ) : null}
      </ConfirmDialog>
    </div>
  );
}
