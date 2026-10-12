import {
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  cn,
  focusRing,
} from "@fettlr/design-system";
import { RiEyeLine, RiEyeOffLine, RiLoader4Line, RiAddLine, RiCloseLine } from "@remixicon/react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { commands, type UiMcpConfigInput, type UiMcpOrigin } from "../../bindings";
import { useFettlr } from "../../store";
import { OriginTag } from "../badges";
import { projectLabel } from "../projects";
import { mcpServerId, originsEqual, revealEndpoint, revealValues } from "./mcpConfig";

type OriginKind = "user" | "projectFile" | "local";
type Transport = "stdio" | "http" | "sse";

type KeyValueRow = { id: string; key: string; value: string };

type Draft = {
  name: string;
  originKind: OriginKind;
  projectPath: string;
  transport: Transport;
  command: string;
  argsText: string;
  url: string;
  env: KeyValueRow[];
  headers: KeyValueRow[];
};

let rowSeq = 0;
function newRow(key = "", value = ""): KeyValueRow {
  rowSeq += 1;
  return { id: `row-${rowSeq}`, key, value };
}

function rowsFrom(values: Record<string, string>): KeyValueRow[] {
  return Object.keys(values)
    .sort()
    .map((key) => newRow(key, values[key]));
}

function emptyDraft(): Draft {
  return {
    name: "",
    originKind: "user",
    projectPath: "",
    transport: "stdio",
    command: "",
    argsText: "",
    url: "",
    env: [],
    headers: [],
  };
}

function originKindOf(origin: UiMcpOrigin): OriginKind {
  switch (origin.kind) {
    case "projectFile":
      return "projectFile";
    case "localScope":
      return "local";
    default:
      return "user";
  }
}

function originProject(origin: UiMcpOrigin): string | null {
  return origin.kind === "localScope" || origin.kind === "projectFile" ? origin.project : null;
}

function parseArgs(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

function dictionaryFrom(rows: KeyValueRow[]): Record<string, string> | null {
  const result: Record<string, string> = {};
  for (const row of rows) {
    const key = row.key.trim();
    if (!key) continue;
    result[key] = row.value;
  }
  return Object.keys(result).length > 0 ? result : null;
}

function scopeHintKey(kind: OriginKind): string {
  switch (kind) {
    case "user":
      return "mcp.form.scopeUserHint";
    case "projectFile":
      return "mcp.form.scopeProjectFileHint";
    case "local":
      return "mcp.form.scopeLocalHint";
  }
}

export function McpFormDialog({
  open,
  serverId,
  onOpenChange,
  onSaved,
}: {
  open: boolean;
  serverId: string | null;
  onOpenChange: (open: boolean) => void;
  onSaved: (id: string) => void;
}) {
  const { t } = useTranslation();
  const snapshot = useFettlr((state) => state.snapshot);
  const dispatch = useFettlr((state) => state.dispatch);

  const server = serverId
    ? (snapshot?.mcpServers.find((candidate) => candidate.id === serverId) ?? null)
    : null;
  const isEditing = serverId !== null;

  const [draft, setDraft] = useState<Draft>(emptyDraft());
  const [loadingSecrets, setLoadingSecrets] = useState(false);
  const [revealFailed, setRevealFailed] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const initializedForRef = useRef<string | null>(null);

  useEffect(() => {
    const key = open ? (serverId ?? "__add__") : null;
    if (key === initializedForRef.current) return;
    initializedForRef.current = key;
    if (!open) return;

    // Read through the store rather than closing over the rendered value: `server` is a fresh
    // object on every snapshot, and listing it as a dependency let a snapshot arriving mid-reveal
    // cancel the run that would have cleared the loading flag, stranding the form on its spinner.
    const server = serverId
      ? (useFettlr.getState().snapshot?.mcpServers.find((c) => c.id === serverId) ?? null)
      : null;

    let cancelled = false;
    void (async () => {
      setDraft(emptyDraft());
      if (!server) {
        setLoadingSecrets(false);
        setRevealFailed(false);
        return;
      }

      setLoadingSecrets(true);
      setRevealFailed(false);
      const env = await revealValues(dispatch, server.id, "env", server.config.envKeys);
      const headers = env
        ? await revealValues(dispatch, server.id, "headers", server.config.headerKeys)
        : null;
      const endpoint = headers ? await revealEndpoint(dispatch, server) : null;
      if (cancelled) return;
      if (env === null || headers === null || endpoint === null) {
        setRevealFailed(true);
        setLoadingSecrets(false);
        return;
      }
      const knownTransports: Transport[] = ["stdio", "http", "sse"];
      setDraft({
        name: server.name,
        originKind: originKindOf(server.origin),
        projectPath: originProject(server.origin) ?? "",
        transport: knownTransports.includes(server.transport as Transport)
          ? (server.transport as Transport)
          : server.config.url
            ? "http"
            : "stdio",
        command: server.config.command ?? "",
        argsText: endpoint.args.join("\n"),
        url: endpoint.url ?? "",
        env: rowsFrom(env),
        headers: rowsFrom(headers),
      });
      setLoadingSecrets(false);
    })();
    return () => {
      cancelled = true;
    };
  }, [open, serverId, dispatch]);

  const projectOptions = useMemo(() => {
    const paths = new Set<string>();
    for (const project of snapshot?.projects ?? []) paths.add(project.path);
    for (const candidate of snapshot?.mcpServers ?? []) {
      if (candidate.origin.kind === "localScope" || candidate.origin.kind === "projectFile") {
        paths.add(candidate.origin.project);
      }
    }
    return Array.from(paths).sort((a, b) => projectLabel(a).localeCompare(projectLabel(b)));
  }, [snapshot]);

  const trimmedName = draft.name.trim();
  const trimmedCommand = draft.command.trim();
  const trimmedUrl = draft.url.trim();
  const isStdio = draft.transport === "stdio";

  const origin: UiMcpOrigin | null = isEditing
    ? (server?.origin ?? null)
    : draft.originKind === "user"
      ? { kind: "userScope" }
      : draft.projectPath
        ? { kind: draft.originKind === "local" ? "localScope" : "projectFile", project: draft.projectPath }
        : null;

  const needsProject = !isEditing && draft.originKind !== "user";
  const projectMissing = needsProject && !draft.projectPath;

  const isDuplicate = origin
    ? (snapshot?.mcpServers ?? []).some(
        (candidate) =>
          originsEqual(candidate.origin, origin) &&
          candidate.name === trimmedName &&
          candidate.id !== (server?.id ?? null),
      )
    : false;

  const hasEndpoint = isStdio ? trimmedCommand.length > 0 : trimmedUrl.length > 0;
  const isComplete = trimmedName.length > 0 && hasEndpoint;
  const problemKey = isDuplicate
    ? "mcp.form.duplicateError"
    : projectMissing
      ? "mcp.form.projectRequiredError"
      : null;
  const canSave =
    isComplete && problemKey === null && origin !== null && !revealFailed && !loadingSecrets;

  const handleSave = async () => {
    if (!canSave || !origin) return;
    setSubmitting(true);
    try {
      const config: UiMcpConfigInput = isStdio
        ? {
            kind: draft.transport,
            command: trimmedCommand,
            args: parseArgs(draft.argsText),
            url: null,
            env: dictionaryFrom(draft.env),
            headers: null,
          }
        : {
            kind: draft.transport,
            command: null,
            args: null,
            url: trimmedUrl,
            env: null,
            headers: dictionaryFrom(draft.headers),
          };
      const input = { name: trimmedName, origin, config };
      const result =
        isEditing && server
          ? await dispatch(commands.editMcpServer(server.id, input))
          : await dispatch(commands.addMcpServer(input));
      if (result) {
        onSaved(mcpServerId(origin, trimmedName));
        onOpenChange(false);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!submitting) onOpenChange(next);
      }}
    >
      <DialogContent closeLabel={t("common.close")} className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{t(isEditing ? "mcp.form.editTitle" : "mcp.form.addTitle")}</DialogTitle>
        </DialogHeader>

        {/* The footer travels with the form rather than being pinned, so it lives inside the
            scrolling box. A plain overflow container stands in for ScrollArea here: ScrollArea's
            viewport sizes itself with a percentage height, which cannot resolve against a parent
            bounded only by max-height. */}
        <div className="min-h-0 flex-1 overflow-y-auto">
          {loadingSecrets ? (
            <DialogBody className="py-8">
              <div className="flex items-center gap-2 text-sm text-text-muted">
                <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" />
                {t("mcp.form.loadingSecrets")}
              </div>
            </DialogBody>
          ) : revealFailed ? (
            <DialogBody className="py-8">
              <p className="text-sm text-text-muted">{t("mcp.form.revealError")}</p>
            </DialogBody>
          ) : (
            <form
              className="flex flex-col gap-5 px-5 pt-5"
              onSubmit={(event) => {
                event.preventDefault();
                void handleSave();
              }}
            >
              <div className="flex flex-col gap-2">
                <Label htmlFor="mcp-name">{t("mcp.form.nameLabel")}</Label>
                <Input
                  id="mcp-name"
                  value={draft.name}
                  onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
                  placeholder={t("mcp.form.namePlaceholder")}
                  disabled={submitting}
                  autoFocus
                />
              </div>

              <div className="flex flex-col gap-2">
                <Label>{t("mcp.form.scopeLabel")}</Label>
                {isEditing && server ? (
                  <>
                    <OriginTag origin={server.origin} />
                    <p className="text-xs text-text-muted">{t("mcp.form.editNote")}</p>
                  </>
                ) : (
                  <>
                    <Select
                      value={draft.originKind}
                      onValueChange={(value) =>
                        setDraft((current) => ({
                          ...current,
                          originKind: value as OriginKind,
                          projectPath: "",
                        }))
                      }
                      disabled={submitting}
                    >
                      <SelectTrigger aria-label={t("mcp.form.scopeLabel")}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="user">{t("mcp.originUser")}</SelectItem>
                        <SelectItem value="projectFile">{t("mcp.originProjectFile")}</SelectItem>
                        <SelectItem value="local">{t("mcp.originLocal")}</SelectItem>
                      </SelectContent>
                    </Select>
                    <p className="text-xs text-text-muted">{t(scopeHintKey(draft.originKind))}</p>
                    {draft.originKind !== "user" ? (
                      <Select
                        value={draft.projectPath}
                        onValueChange={(value) =>
                          setDraft((current) => ({ ...current, projectPath: value }))
                        }
                        disabled={submitting}
                      >
                        <SelectTrigger aria-label={t("mcp.form.projectPlaceholder")}>
                          <SelectValue placeholder={t("mcp.form.projectPlaceholder")} />
                        </SelectTrigger>
                        <SelectContent>
                          {projectOptions.map((path) => (
                            <SelectItem key={path} value={path}>
                              {projectLabel(path)}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    ) : null}
                  </>
                )}
              </div>

              <div className="flex flex-col gap-2">
                <Label>{t("mcp.form.transportLabel")}</Label>
                <Select
                  value={draft.transport}
                  onValueChange={(value) =>
                    setDraft((current) => ({ ...current, transport: value as Transport }))
                  }
                  disabled={submitting}
                >
                  <SelectTrigger aria-label={t("mcp.form.transportLabel")}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="stdio">{t("mcp.form.transportStdio")}</SelectItem>
                    <SelectItem value="http">{t("mcp.form.transportHttp")}</SelectItem>
                    <SelectItem value="sse">{t("mcp.form.transportSse")}</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {isStdio ? (
                <>
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="mcp-command">{t("mcp.form.commandLabel")}</Label>
                    <Input
                      id="mcp-command"
                      value={draft.command}
                      onChange={(event) =>
                        setDraft((current) => ({ ...current, command: event.target.value }))
                      }
                      placeholder={t("mcp.form.commandPlaceholder")}
                      disabled={submitting}
                    />
                  </div>
                  <div className="flex flex-col gap-1.5">
                    <Label htmlFor="mcp-args">{t("mcp.form.argumentsLabel")}</Label>
                    <textarea
                      id="mcp-args"
                      value={draft.argsText}
                      onChange={(event) =>
                        setDraft((current) => ({ ...current, argsText: event.target.value }))
                      }
                      rows={4}
                      disabled={submitting}
                      className={cn(
                        "w-full rounded-md border border-border-strong bg-surface px-2.5 py-1.5",
                        "font-mono text-sm text-text transition-colors",
                        "placeholder:text-text-muted disabled:cursor-not-allowed disabled:opacity-45",
                        focusRing,
                      )}
                    />
                    <p className="text-xs text-text-muted">{t("mcp.form.argumentsHint")}</p>
                  </div>
                  <KeyValueEditor
                    title={t("mcp.form.environmentLabel")}
                    rows={draft.env}
                    onChange={(rows) => setDraft((current) => ({ ...current, env: rows }))}
                    addLabel={t("mcp.form.addVariable")}
                    keyPlaceholder={t("mcp.form.keyPlaceholder")}
                    valuePlaceholder={t("mcp.form.valuePlaceholder")}
                    disabled={submitting}
                  />
                </>
              ) : (
                <>
                  <div className="flex flex-col gap-2">
                    <Label htmlFor="mcp-url">{t("mcp.form.urlLabel")}</Label>
                    <Input
                      id="mcp-url"
                      value={draft.url}
                      onChange={(event) => setDraft((current) => ({ ...current, url: event.target.value }))}
                      placeholder={t("mcp.form.urlPlaceholder")}
                      disabled={submitting}
                    />
                  </div>
                  <KeyValueEditor
                    title={t("mcp.form.headersLabel")}
                    rows={draft.headers}
                    onChange={(rows) => setDraft((current) => ({ ...current, headers: rows }))}
                    addLabel={t("mcp.form.addHeader")}
                    keyPlaceholder={t("mcp.form.headerKeyPlaceholder")}
                    valuePlaceholder={t("mcp.form.valuePlaceholder")}
                    disabled={submitting}
                  />
                </>
              )}

              {problemKey ? <p className="text-xs text-danger-text">{t(problemKey)}</p> : null}
            </form>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="secondary"
              onClick={() => onOpenChange(false)}
              disabled={submitting}
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              disabled={submitting || loadingSecrets || !canSave}
              onClick={() => void handleSave()}
            >
              {submitting ? <RiLoader4Line className="size-4 animate-spin" aria-hidden="true" /> : null}
              {t("mcp.form.save")}
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function KeyValueEditor({
  title,
  rows,
  onChange,
  addLabel,
  keyPlaceholder,
  valuePlaceholder,
  disabled,
}: {
  title: string;
  rows: KeyValueRow[];
  onChange: (rows: KeyValueRow[]) => void;
  addLabel: string;
  keyPlaceholder: string;
  valuePlaceholder: string;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  // Every value this editor holds is an environment value or a header value, so it starts hidden
  // and is shown one row at a time, the same way the detail pane shows them.
  const [revealed, setRevealed] = useState<ReadonlySet<string>>(() => new Set());
  return (
    <div className="flex flex-col gap-2">
      <Label>{title}</Label>
      <div className="flex flex-col gap-1.5">
        {rows.map((row, index) => {
          const isRevealed = revealed.has(row.id);
          return (
          <div key={row.id} className="flex items-center gap-1.5">
            <Input
              value={row.key}
              onChange={(event) =>
                onChange(rows.map((r) => (r.id === row.id ? { ...r, key: event.target.value } : r)))
              }
              placeholder={keyPlaceholder}
              disabled={disabled}
              className="w-36 shrink-0"
              aria-label={t("mcp.form.keyAria", { index: index + 1 })}
            />
            <Input
              type={isRevealed ? "text" : "password"}
              value={row.value}
              onChange={(event) =>
                onChange(rows.map((r) => (r.id === row.id ? { ...r, value: event.target.value } : r)))
              }
              placeholder={valuePlaceholder}
              disabled={disabled}
              aria-label={t("mcp.form.valueAria", { index: index + 1 })}
            />
            <button
              type="button"
              onClick={() =>
                setRevealed((current) => {
                  const next = new Set(current);
                  if (!next.delete(row.id)) next.add(row.id);
                  return next;
                })
              }
              disabled={disabled}
              aria-label={
                row.key
                  ? t(isRevealed ? "mcp.detail.hideAria" : "mcp.detail.revealAria", { key: row.key })
                  : t("mcp.form.revealRowAria", { index: index + 1 })
              }
              className={cn(
                "inline-flex size-9 shrink-0 items-center justify-center rounded-md text-text-muted",
                "transition-colors hover:bg-surface-hover hover:text-text",
                "disabled:pointer-events-none disabled:opacity-45",
                focusRing,
              )}
            >
              {isRevealed ? (
                <RiEyeOffLine className="size-4" aria-hidden="true" />
              ) : (
                <RiEyeLine className="size-4" aria-hidden="true" />
              )}
            </button>
            <button
              type="button"
              onClick={() => onChange(rows.filter((r) => r.id !== row.id))}
              disabled={disabled}
              aria-label={t("mcp.form.removeRowAria")}
              className={cn(
                "inline-flex size-9 shrink-0 items-center justify-center rounded-md text-text-muted",
                "transition-colors hover:bg-surface-hover hover:text-text",
                "disabled:pointer-events-none disabled:opacity-45",
                focusRing,
              )}
            >
              <RiCloseLine className="size-4" aria-hidden="true" />
            </button>
          </div>
          );
        })}
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled}
          onClick={() => onChange([...rows, newRow()])}
          className="self-start"
        >
          <RiAddLine className="size-3.5" aria-hidden="true" />
          {addLabel}
        </Button>
      </div>
    </div>
  );
}
