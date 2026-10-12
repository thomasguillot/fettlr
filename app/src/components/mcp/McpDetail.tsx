import {
  Badge,
  Button,
  Checkbox,
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  ScrollArea,
  cn,
  focusRing,
} from "@fettlr/design-system";
import { RiArrowUpCircleLine, RiEyeLine, RiEyeOffLine, RiLoader4Line, RiPencilLine, RiServerLine, RiDeleteBinLine } from "@remixicon/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiMcpServer, type UiPrefs } from "../../bindings";
import { staleIds } from "../../sections";
import { useFettlr } from "../../store";
import { useUpdateAction } from "../../useUpdateAction";
import { OriginTag, UpdateStatusBadge } from "../badges";
import { ConfirmDialog } from "../ConfirmDialog";
import { DetailEmpty, DetailHeader, DetailRow, codeChip } from "../screen";
import { projectLabel } from "../projects";
import { repinMcpServer } from "./mcpConfig";

type RevealField = "env" | "headers" | "args" | "url";

function mcpDetailsName(server: UiMcpServer): string {
  if (server.origin.kind !== "plugin") return server.name;
  const pluginId = server.origin.pluginId;
  const at = pluginId.lastIndexOf("@");
  const pluginName = at === -1 ? pluginId : pluginId.slice(0, at);
  return `plugin:${pluginName}:${server.name}`;
}

function mcpDetailsProject(server: UiMcpServer): string | null {
  return server.origin.kind === "localScope" || server.origin.kind === "projectFile"
    ? server.origin.project
    : null;
}

export function McpDetail({
  serverId,
  open = true,
  onEdit,
  onRemoved,
}: {
  serverId: string | null;
  open?: boolean;
  onEdit: (id: string) => void;
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);

  const server = serverId
    ? (snapshot?.mcpServers.find((candidate) => candidate.id === serverId) ?? null)
    : null;

  if (!server) {
    return (
      <DetailEmpty icon={RiServerLine} message={t("mcp.detail.empty")} />
    );
  }

  return <ActiveMcpDetail key={server.id} server={server} open={open} onEdit={onEdit} onRemoved={onRemoved} />;
}

function ActiveMcpDetail({
  server,
  open,
  onEdit,
  onRemoved,
}: {
  server: UiMcpServer;
  open: boolean;
  onEdit: (id: string) => void;
  onRemoved: () => void;
}) {
  const { t } = useTranslation();
  const dispatch = useFettlr((state) => state.dispatch);
  const select = useFettlr((state) => state.select);
  const busy = useFettlr((state) => state.busy);
  const prefs = useFettlr((state) => state.prefs);
  const savePrefs = useFettlr((state) => state.savePrefs);
  const { run, isRunning } = useUpdateAction();
  const updateReport = useFettlr((state) => state.snapshot?.updateReport ?? null);

  const [working, setWorking] = useState(false);
  const [promoting, setPromoting] = useState(false);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [promoteOpen, setPromoteOpen] = useState(false);
  const [outputOpen, setOutputOpen] = useState(false);
  const [revealed, setRevealed] = useState<Record<string, string>>({});

  const [revealScope, setRevealScope] = useState({ config: JSON.stringify(server.config), open });
  const configKey = JSON.stringify(server.config);
  if (revealScope.config !== configKey || revealScope.open !== open) {
    setRevealScope({ config: configKey, open });
    setRevealed({});
  }

  const isPlugin = server.origin.kind === "plugin";
  const pluginId = server.origin.kind === "plugin" ? server.origin.pluginId : null;
  const updateState = updateReport?.mcpServers[server.id];
  const stale = staleIds(updateReport, "mcpServers").has(server.id);

  const handleToggleEnabled = async () => {
    setWorking(true);
    try {
      await dispatch(commands.setMcpServerEnabled(server.id, !server.isEnabled));
    } finally {
      setWorking(false);
    }
  };

  const handleRemove = () => {
    void dispatch(commands.removeMcpServer(server.id)).then((result) => {
      if (result) onRemoved();
    });
  };

  const runPromote = async () => {
    setPromoting(true);
    try {
      const result = await dispatch(commands.promoteMcpServer(server.id));
      if (result && !result.mcpServers.some((candidate) => candidate.id === server.id)) {
        onRemoved();
      }
    } finally {
      setPromoting(false);
    }
  };

  const handlePromoteClick = () => {
    if (prefs?.confirmMcpPromotion === false) {
      void runPromote();
      return;
    }
    setPromoteOpen(true);
  };

  const updating = isRunning(server.id);
  const handleUpdate = async () => {
    if (updateState?.kind !== "updateAvailable") return;
    await run(server.id, () => repinMcpServer(dispatch, server, updateState.latest));
  };

  const revealField = async (field: RevealField, key: string) => {
    const expected =
      field === "args" ? (server.config.args[Number(key)] ?? null) : field === "url" ? server.config.url : null;
    const value = await dispatch(commands.revealSecret(server.id, field, key, expected));
    if (value !== undefined) {
      setRevealed((previous) => ({ ...previous, [`${field}:${key}`]: value }));
    }
  };

  const hideField = (field: RevealField, key: string) => {
    setRevealed((previous) => {
      const next = { ...previous };
      delete next[`${field}:${key}`];
      return next;
    });
  };

  return (
    <div className="flex flex-1 flex-col gap-6 px-6 pb-6">
      <DetailHeader title={server.name} />

      <div className="flex flex-wrap items-center gap-1.5">
        <OriginTag origin={server.origin} />
        <Badge tone="neutral" variant="soft">
          {server.transport}
        </Badge>
        {!server.isEnabled ? (
          <Badge tone="neutral" variant="soft">
            {t("common.disabled")}
          </Badge>
        ) : null}
      </div>

      {isPlugin ? (
        <div className="flex items-center justify-between gap-3 rounded-md border border-border bg-bg-subtle p-3">
          <p className="text-sm text-text-muted">{t("mcp.detail.pluginManaged", { pluginId })}</p>
          <Button type="button" variant="secondary" size="sm" onClick={() => select("plugins")}>
            {t("mcp.detail.jumpToPlugin")}
          </Button>
        </div>
      ) : null}

      <dl className="flex flex-col gap-4">
        {server.config.command ? (
          <DetailRow label={t("mcp.detail.command")}>
            <code className={codeChip}>
              {server.config.command}
            </code>
          </DetailRow>
        ) : null}
        {server.config.args.length > 0 ? (
          <DetailRow label={t("mcp.detail.arguments")}>
            <div className="flex flex-col gap-0.5">
              {server.config.args.map((argument, index) => {
                const key = String(index);
                const isMasked = server.config.maskedArgs.includes(index);
                const shown = revealed[`args:${key}`];
                return (
                  <div key={index} className="flex items-center gap-1.5">
                    <code className={cn(codeChip, "min-w-0 flex-1 truncate")}>
                      {shown ?? argument}
                    </code>
                    {isMasked ? (
                      <RevealButton
                        revealed={shown !== undefined}
                        label={t(shown !== undefined ? "mcp.detail.hideArgumentAria" : "mcp.detail.revealArgumentAria", {
                          index: index + 1,
                        })}
                        onToggle={() => (shown !== undefined ? hideField("args", key) : void revealField("args", key))}
                      />
                    ) : null}
                  </div>
                );
              })}
            </div>
          </DetailRow>
        ) : null}
        {server.config.url ? (
          <DetailRow label={t("mcp.detail.url")}>
            <div className="flex items-center gap-1.5">
              <code className={cn(codeChip, "min-w-0 flex-1 truncate")}>
                {revealed["url:"] ?? server.config.url}
              </code>
              {server.config.urlMasked ? (
                <RevealButton
                  revealed={revealed["url:"] !== undefined}
                  label={t(revealed["url:"] !== undefined ? "mcp.detail.hideUrlAria" : "mcp.detail.revealUrlAria")}
                  onToggle={() => (revealed["url:"] !== undefined ? hideField("url", "") : void revealField("url", ""))}
                />
              ) : null}
            </div>
          </DetailRow>
        ) : null}
        {server.config.envKeys.length > 0 ? (
          <DetailRow label={t("mcp.detail.environment")}>
            <SecretList
              field="env"
              values={server.config.env}
              revealed={revealed}
              onReveal={revealField}
              onHide={hideField}
            />
          </DetailRow>
        ) : null}
        {server.config.headerKeys.length > 0 ? (
          <DetailRow label={t("mcp.detail.headers")}>
            <SecretList
              field="headers"
              values={server.config.headers}
              revealed={revealed}
              onReveal={revealField}
              onHide={hideField}
            />
          </DetailRow>
        ) : null}
        {server.origin.kind === "localScope" || server.origin.kind === "projectFile" ? (
          <DetailRow label={t("mcp.detail.project")}>
            <code className={codeChip}>
              {server.origin.project}
            </code>
          </DetailRow>
        ) : null}
        {isPlugin && pluginId ? (
          <DetailRow label={t("mcp.detail.plugin")}>
            <span className="font-mono text-xs text-text-muted">{pluginId}</span>
          </DetailRow>
        ) : null}
        <DetailRow label={t("mcp.detail.updateStatus")}>
          <UpdateStatusBadge state={updateState} pending="updateAvailable" tracksLatest />
        </DetailRow>
      </dl>

      <div className="mt-auto grid grid-cols-2 gap-2">
        {isPlugin ? null : (
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
            {t("mcp.detail.update")}
          </Button>
        )}
        {!isPlugin ? (
          <Button
            type="button"
            variant="secondary"
            disabled={busy || working}
            onClick={() => void handleToggleEnabled()}
          >
            {server.isEnabled ? (
              <RiEyeOffLine className="size-4" aria-hidden="true" />
            ) : (
              <RiEyeLine className="size-4" aria-hidden="true" />
            )}
            {t(server.isEnabled ? "mcp.detail.disable" : "mcp.detail.enable")}
          </Button>
        ) : null}
        {!isPlugin ? (
          <Button type="button" variant="secondary" disabled={busy} onClick={() => onEdit(server.id)}>
            <RiPencilLine className="size-4" aria-hidden="true" />
            {t("mcp.detail.edit")}
          </Button>
        ) : null}
        {server.origin.kind === "localScope" && server.isEnabled ? (
          <Button
            type="button"
            variant="secondary"
            disabled={busy || promoting}
            onClick={handlePromoteClick}
          >
            {t("mcp.detail.promote")}
          </Button>
        ) : null}
        <Button type="button" variant="secondary" disabled={busy} onClick={() => setOutputOpen(true)}>
          {t("mcp.detail.details")}
        </Button>
        {!isPlugin ? (
          <Button type="button" variant="dangerGhost" disabled={busy} onClick={() => setConfirmOpen(true)}>
            <RiDeleteBinLine className="size-4" aria-hidden="true" />
            {t("mcp.detail.remove")}
          </Button>
        ) : null}
      </div>

      <ConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t("mcp.detail.removeTitle")}
        description={t("mcp.detail.removeDescription", { name: server.name })}
        confirmLabel={t("mcp.detail.remove")}
        onConfirm={handleRemove}
      />

      {server.origin.kind === "localScope" ? (
        <PromoteConfirmDialog
          open={promoteOpen}
          onOpenChange={setPromoteOpen}
          name={server.name}
          project={projectLabel(server.origin.project)}
          prefs={prefs}
          savePrefs={savePrefs}
          onConfirm={() => {
            setPromoteOpen(false);
            void runPromote();
          }}
        />
      ) : null}

      <McpOutputDialog open={outputOpen} onOpenChange={setOutputOpen} server={server} />
    </div>
  );
}

function PromoteConfirmDialog({
  open,
  onOpenChange,
  name,
  project,
  prefs,
  savePrefs,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  name: string;
  project: string;
  prefs: UiPrefs | null;
  savePrefs: (next: UiPrefs) => Promise<boolean>;
  onConfirm: () => void;
}) {
  const { t } = useTranslation();
  const [dontAsk, setDontAsk] = useState(false);

  return (
    // The dialog stays mounted for any local-scope server, so a box ticked and then cancelled would
    // otherwise still be ticked on the next open, and confirming would write a preference the reader
    // never chose in that interaction.
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) setDontAsk(false);
        onOpenChange(next);
      }}
    >
      <DialogContent closeLabel={t("common.close")}>
        <DialogHeader>
          <DialogTitle>{t("mcp.detail.promoteTitle")}</DialogTitle>
          <DialogDescription>
            {t("mcp.detail.promoteDescription", { name, project })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="justify-between">
          <label className="flex items-center gap-2 text-sm text-text-muted">
            <Checkbox
              checked={dontAsk}
              onCheckedChange={(checked) => setDontAsk(checked === true)}
            />
            {t("mcp.detail.promoteDontAsk")}
          </label>
          <div className="flex items-center gap-2">
            <Button type="button" variant="secondary" onClick={() => onOpenChange(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              onClick={() => {
                void (async () => {
                  if (dontAsk && prefs && !(await savePrefs({ ...prefs, confirmMcpPromotion: false }))) {
                    return;
                  }
                  onConfirm();
                })();
              }}
            >
              {t("mcp.detail.promoteConfirm")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SecretList({
  field,
  values,
  revealed,
  onReveal,
  onHide,
}: {
  field: "env" | "headers";
  values: Record<string, string>;
  revealed: Record<string, string>;
  onReveal: (field: RevealField, key: string) => void;
  onHide: (field: RevealField, key: string) => void;
}) {
  const { t } = useTranslation();
  const keys = Object.keys(values).sort();
  return (
    <ul className="flex flex-col gap-1">
      {keys.map((key) => {
        const revealedValue = revealed[`${field}:${key}`];
        const isRevealed = revealedValue !== undefined;
        return (
          <li key={key} className="flex items-center gap-1.5">
            <code className="shrink-0 rounded bg-bg-subtle px-1.5 py-0.5 font-mono text-xs text-text-muted">
              {key}
            </code>
            <code className="min-w-0 flex-1 truncate rounded bg-bg-subtle px-1.5 py-0.5 font-mono text-xs text-text-muted">
              {isRevealed ? revealedValue : values[key]}
            </code>
            <RevealButton
              revealed={isRevealed}
              label={t(isRevealed ? "mcp.detail.hideAria" : "mcp.detail.revealAria", { key })}
              onToggle={() => (isRevealed ? onHide(field, key) : onReveal(field, key))}
            />
          </li>
        );
      })}
    </ul>
  );
}

function RevealButton({
  revealed,
  label,
  onToggle,
}: {
  revealed: boolean;
  label: string;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-label={label}
      className={cn(
        "inline-flex size-6 shrink-0 items-center justify-center rounded-md text-text-muted",
        "transition-colors hover:bg-surface-hover hover:text-text",
        focusRing,
      )}
    >
      {revealed ? (
        <RiEyeOffLine className="size-3.5" aria-hidden="true" />
      ) : (
        <RiEyeLine className="size-3.5" aria-hidden="true" />
      )}
    </button>
  );
}

function McpOutputDialog({
  open,
  onOpenChange,
  server,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  server: UiMcpServer;
}) {
  const { t } = useTranslation();
  const [output, setOutput] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  const detailsName = mcpDetailsName(server);
  const detailsProject = mcpDetailsProject(server);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void (async () => {
      setOutput(null);
      setFailed(false);
      const result = await commands
        .mcpDetails(detailsName, detailsProject)
        .catch(() => ({ status: "error" }) as const);
      if (cancelled) return;
      if (result.status === "ok") setOutput(result.data);
      else setFailed(true);
    })();
    return () => {
      cancelled = true;
    };
  }, [open, detailsName, detailsProject]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent closeLabel={t("common.close")}>
        <DialogHeader>
          <DialogTitle>{t("mcp.detail.outputTitle")}</DialogTitle>
        </DialogHeader>
        <DialogBody>
          <ScrollArea className="h-64 rounded-md border border-border bg-bg-subtle">
          {failed ? (
            <p className="p-3 text-sm text-text-muted">{t("mcp.detail.outputError")}</p>
          ) : output === null ? (
            <div className="flex items-center gap-2 p-3 text-sm text-text-muted">
              <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
              {t("mcp.detail.outputLoading")}
            </div>
          ) : output.trim().length === 0 ? (
            <p className="p-3 text-sm text-text-muted">{t("mcp.detail.outputEmpty")}</p>
          ) : (
            <pre className="p-3 font-mono text-xs whitespace-pre-wrap break-words text-text-muted">
              {output}
              </pre>
            )}
          </ScrollArea>
        </DialogBody>
        <DialogFooter>
          <Button type="button" variant="secondary" onClick={() => onOpenChange(false)}>
            {t("common.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
