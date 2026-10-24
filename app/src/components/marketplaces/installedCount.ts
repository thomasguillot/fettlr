import type { UiPlugin } from "../../bindings";

export function installedFromMarketplace(plugins: readonly UiPlugin[], marketplaceName: string): number {
  return plugins.filter((plugin) => plugin.marketplaceName === marketplaceName && !plugin.isAvailableOnly).length;
}
