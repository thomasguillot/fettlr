import type {
  UiMarketplace,
  UiMcpOrigin,
  UiMcpServer,
  UiParkedSkill,
  UiParkedSubagent,
  UiPlugin,
  UiSkill,
  UiSubagent,
} from "../bindings";

export function matchesText(value: string, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === "") return true;
  return value.toLowerCase().includes(needle);
}

function originSearchText(origin: UiMcpOrigin): string {
  switch (origin.kind) {
    case "userScope":
      return "";
    case "localScope":
    case "projectFile":
      return origin.project;
    case "plugin":
      return origin.pluginId;
  }
}

export function skillSearchValue(skill: UiSkill): string {
  return [skill.name, skill.title ?? "", skill.summary ?? "", skill.id].join(" ");
}

export function parkedSearchValue(parked: UiParkedSkill): string {
  return [parked.name, parked.id].join(" ");
}

export function subagentSearchValue(subagent: UiSubagent): string {
  return [subagent.name, subagent.summary ?? "", subagent.model ?? "", subagent.id].join(" ");
}

export function parkedSubagentSearchValue(parked: UiParkedSubagent): string {
  return [parked.name, parked.id].join(" ");
}

export function pluginSearchValue(plugin: UiPlugin): string {
  return [plugin.name, plugin.pluginId, plugin.marketplaceName, plugin.description ?? "", plugin.id].join(" ");
}

export function marketplaceSearchValue(marketplace: UiMarketplace): string {
  return [marketplace.name, marketplace.source, marketplace.displaySource].join(" ");
}

export function mcpSearchValue(server: UiMcpServer): string {
  return [server.name, server.transport, originSearchText(server.origin), server.id].join(" ");
}
