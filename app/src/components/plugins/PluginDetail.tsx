import { Badge, Button, ScrollArea } from "@fettlr/design-system";
import { RiPuzzleLine, RiArrowUpCircleLine, RiLoader4Line, RiDeleteBinLine } from "@remixicon/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiPlugin } from "../../bindings";
import { staleIds } from "../../sections";
import { useFettlr } from "../../store";
import { useUpdateAction } from "../../useUpdateAction";
import { ScopeBadge, UpdateStatusBadge } from "../badges";
import { ConfirmDialog } from "../ConfirmDialog";
import { DetailEmpty, DetailHeader, DetailRow, UsageRow, capsHeading, codeChip } from "../screen";

export function PluginDetail({
  pluginId,
  onRemoved,
}: {
  pluginId: string | null;
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const plugin = useFettlr((state) =>
    pluginId ? (state.snapshot?.plugins.find((candidate) => candidate.id === pluginId) ?? null) : null,
  );

  if (!plugin) {
    return (
      <DetailEmpty icon={RiPuzzleLine} message={t("plugins.detail.empty")} />
    );
  }

  return <ActivePluginDetail key={plugin.id} plugin={plugin} onRemoved={onRemoved} />;
}

function ActivePluginDetail({ plugin, onRemoved }: { plugin: UiPlugin; onRemoved: () => void }) {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);
  const dispatch = useFettlr((state) => state.dispatch);
  const busy = useFettlr((state) => state.busy);
  const { run, isRunning } = useUpdateAction();
  const [fetched, setFetched] = useState<{ pluginId: string; details: string | null; error: boolean }>({
    pluginId: "",
    details: null,
    error: false,
  });
  const [confirmOpen, setConfirmOpen] = useState(false);

  const rawPluginId = plugin.pluginId;

  // Off dispatch: selecting a row is not a request for this, so it must not disable the window or
  // raise the error dialog over the inline message this pane already shows.
  useEffect(() => {
    let cancelled = false;
    void commands
      .pluginDetails(rawPluginId)
      .then((result) => {
        if (cancelled) return;
        setFetched({
          pluginId: rawPluginId,
          details: result.status === "ok" ? result.data : null,
          error: result.status === "error",
        });
      })
      .catch(() => {
        if (cancelled) return;
        setFetched({ pluginId: rawPluginId, details: null, error: true });
      });
    return () => {
      cancelled = true;
    };
  }, [rawPluginId]);

  const isCurrent = fetched.pluginId === rawPluginId;
  const details = isCurrent ? fetched.details : null;
  const detailsError = isCurrent && fetched.error;

  const updateReport = snapshot?.updateReport ?? null;
  const updateState = updateReport?.plugins[plugin.id];
  const stale = staleIds(updateReport, "plugins").has(plugin.id);

  const pluginSkills = (snapshot?.skills ?? []).filter(
    (skill) => skill.provenance.kind === "plugin" && skill.provenance.pluginId === plugin.pluginId,
  );

  const updating = isRunning(plugin.id);
  const unsupported = plugin.scope.kind === "other";
  const handleUpdate = () => run(plugin.id, () => dispatch(commands.updatePlugin(plugin.pluginId, plugin.scope)));

  return (
    <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
      <DetailHeader title={plugin.name} />

      <p className="font-mono text-sm text-text-muted">{plugin.pluginId}</p>

      <dl className="flex flex-col gap-4">
        <DetailRow label={t("plugins.detail.description")}>
          {plugin.description ?? <span className="text-text-muted">{t("plugins.detail.noDescription")}</span>}
        </DetailRow>
        <DetailRow label={t("plugins.detail.marketplace")}>{plugin.marketplaceName}</DetailRow>
        <DetailRow label={t("plugins.detail.version")}>{plugin.version}</DetailRow>
        <DetailRow label={t("plugins.detail.scope")}>
          <ScopeBadge scope={plugin.scope} />
        </DetailRow>
        <DetailRow label={t("plugins.detail.status")}>
          <Badge tone={plugin.enabled ? "success" : "neutral"} variant="soft">
            {t(plugin.enabled ? "common.enabled" : "common.disabled")}
          </Badge>
        </DetailRow>
        <DetailRow label={t("plugins.detail.updateStatus")}>
          <UpdateStatusBadge state={updateState} pending="updateAvailable" stale={stale} />
        </DetailRow>
        <UsageRow usage={plugin.usage} />
        <DetailRow label={t("plugins.detail.installPath")}>
          <code className={codeChip}>
            {plugin.installPath}
          </code>
        </DetailRow>
      </dl>

      <section className="flex flex-col gap-2">
        <h3 className={capsHeading}>
          {t("plugins.detail.detailsHeading")}
        </h3>
        <ScrollArea className="h-56 rounded-md border border-border bg-bg-subtle">
          {detailsError ? (
            <p className="p-3 text-sm text-text-muted">{t("plugins.detail.detailsError")}</p>
          ) : details === null ? (
            <div className="flex items-center gap-2 p-3 text-sm text-text-muted">
              <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
              {t("plugins.detail.detailsLoading")}
            </div>
          ) : (
            <pre className="p-3 font-mono text-xs whitespace-pre-wrap break-words text-text-muted">
              {details}
            </pre>
          )}
        </ScrollArea>
      </section>

      <section className="flex flex-col gap-2">
        <h3 className={capsHeading}>
          {t("plugins.detail.containedSkills")}
        </h3>
        {pluginSkills.length === 0 ? (
          <p className="text-sm text-text-muted">{t("plugins.detail.noSkills")}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {pluginSkills.map((skill) => (
              <li key={skill.id} className="text-sm text-text">
                {skill.title ?? skill.name}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="flex flex-col gap-2">
        <h3 className={capsHeading}>
          {t("plugins.detail.mcpServers")}
        </h3>
        {plugin.mcpServerNames.length === 0 ? (
          <p className="text-sm text-text-muted">{t("plugins.detail.noMcpServers")}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {plugin.mcpServerNames.map((name) => (
              <li key={name} className="font-mono text-sm text-text">
                {name}
              </li>
            ))}
          </ul>
        )}
      </section>

      <div className="mt-auto grid grid-cols-2 gap-2">
        <Button
          type="button"
          variant="secondary"
          disabled={busy || updating || !stale || unsupported}
          onClick={() => void handleUpdate()}
        >
          {updating ? (
            <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
          ) : (
            <RiArrowUpCircleLine className="size-4" aria-hidden="true" />
          )}
          {t("plugins.detail.update")}
        </Button>
        <Button
          type="button"
          variant="dangerGhost"
          disabled={busy || unsupported}
          onClick={() => setConfirmOpen(true)}
        >
          <RiDeleteBinLine className="size-4" aria-hidden="true" />
          {t("plugins.detail.uninstall")}
        </Button>
      </div>

      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t("plugins.detail.uninstallTitle")}
        description={t("plugins.detail.uninstallDescription", { name: plugin.name })}
        confirmLabel={t("plugins.detail.uninstall")}
        onConfirm={() => {
          void dispatch(commands.uninstallPlugin(plugin.pluginId, plugin.scope)).then((result) => {
            if (result) onRemoved();
          });
        }}
      />
    </div>
  );
}
