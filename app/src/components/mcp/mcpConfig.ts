import i18next from "i18next";

import { commands, type UiMcpOrigin, type UiMcpServer, type UiSecretField, type UiSnapshot } from "../../bindings";
import { type CommandResult, useFettlr } from "../../store";

type Dispatch = <T>(command: Promise<CommandResult<T>>) => Promise<T | undefined>;

function originKey(origin: UiMcpOrigin): string {
  switch (origin.kind) {
    case "userScope":
      return "user";
    case "localScope":
      return `local:${origin.project}`;
    case "projectFile":
      return `project:${origin.project}`;
    case "plugin":
      return `plugin:${origin.pluginId}`;
  }
}

export function mcpServerId(origin: UiMcpOrigin, name: string): string {
  return `${originKey(origin)}/${name}`;
}

export function originsEqual(a: UiMcpOrigin, b: UiMcpOrigin): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "localScope" && b.kind === "localScope") return a.project === b.project;
  if (a.kind === "projectFile" && b.kind === "projectFile") return a.project === b.project;
  if (a.kind === "plugin" && b.kind === "plugin") return a.pluginId === b.pluginId;
  return true;
}

export async function revealValues(
  dispatch: Dispatch,
  serverId: string,
  field: UiSecretField,
  keys: string[],
): Promise<Record<string, string> | null> {
  const values: Record<string, string> = {};
  for (const key of keys) {
    const value = await dispatch(commands.revealSecret(serverId, field, key, null));
    if (value === undefined) return null;
    values[key] = value;
  }
  return values;
}

export async function revealEndpoint(
  dispatch: Dispatch,
  server: UiMcpServer,
): Promise<{ args: string[]; url: string | null } | null> {
  const args = [...server.config.args];
  for (const index of server.config.maskedArgs) {
    const value = await dispatch(commands.revealSecret(server.id, "args", String(index), server.config.args[index]));
    if (value === undefined) return null;
    args[index] = value;
  }
  let url = server.config.url;
  if (server.config.urlMasked) {
    const value = await dispatch(commands.revealSecret(server.id, "url", "", server.config.url));
    if (value === undefined) return null;
    url = value;
  }
  return { args, url };
}

type PackageRegistry = "npm" | "pypi";

function runnerName(command: string): string {
  const segments = command.split(/[\\/]/);
  return segments[segments.length - 1] || command;
}

function registryFor(command: string): PackageRegistry | null {
  switch (runnerName(command).toLowerCase()) {
    case "npx":
    case "bunx":
      return "npm";
    case "uvx":
      return "pypi";
    default:
      return null;
  }
}

function firstPositionalIndex(args: string[]): number | null {
  const index = args.findIndex((arg) => arg.length > 0 && !arg.startsWith("-"));
  return index === -1 ? null : index;
}

function pinnedVersionOf(raw: string): string | null {
  return /^[0-9]/.test(raw) ? raw : null;
}

function pinnedVersionFromToken(registry: PackageRegistry, token: string): string | null {
  if (registry === "pypi") {
    const separator = token.indexOf("==");
    if (separator !== -1) return pinnedVersionOf(token.slice(separator + 2));
  }
  const at = token.lastIndexOf("@");
  return at > 0 ? pinnedVersionOf(token.slice(at + 1)) : null;
}

// Fettlr's core has no repin command (Claude Code has no "update" verb for an MCP server, only a
// config rewrite via edit), so the "Update" action rebuilds the pinned argument itself, mirroring
// crates/fettlr-core/src/updates/package_spec.rs exactly.
export function repinnedArgs(command: string | null, args: string[], latest: string): string[] | null {
  if (!command) return null;
  const registry = registryFor(command);
  if (!registry) return null;
  const index = firstPositionalIndex(args);
  if (index === null) return null;
  const token = args[index];
  const pinned = pinnedVersionFromToken(registry, token);
  if (!pinned || !token.endsWith(pinned)) return null;
  const rewritten = [...args];
  rewritten[index] = token.slice(0, token.length - pinned.length) + latest;
  return rewritten;
}

export async function repinMcpServer(
  dispatch: Dispatch,
  server: UiMcpServer,
  latest: string,
): Promise<UiSnapshot | undefined> {
  const unsupported = () => {
    useFettlr.setState({ error: { message: i18next.t("mcp.repinUnsupported"), detail: null } });
    return undefined;
  };
  if (!repinnedArgs(server.config.command, server.config.args, latest)) return unsupported();
  const endpoint = await revealEndpoint(dispatch, server);
  if (endpoint === null) return undefined;
  const rewritten = repinnedArgs(server.config.command, endpoint.args, latest);
  if (!rewritten) return unsupported();
  const env = await revealValues(dispatch, server.id, "env", server.config.envKeys);
  if (env === null) return undefined;
  const headers = await revealValues(dispatch, server.id, "headers", server.config.headerKeys);
  if (headers === null) return undefined;
  return dispatch(
    commands.editMcpServer(server.id, {
      name: server.name,
      origin: server.origin,
      config: {
        kind: server.config.kind,
        command: server.config.command,
        args: rewritten,
        url: endpoint.url,
        env: Object.keys(env).length > 0 ? env : null,
        headers: Object.keys(headers).length > 0 ? headers : null,
      },
    }),
  );
}
