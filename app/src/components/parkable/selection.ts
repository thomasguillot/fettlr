export type Selection<K extends string> = { kind: K | "parked"; id: string };

export type SkillSelection = Selection<"skill">;
export type SubagentSelection = Selection<"subagent">;

export function itemKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}

export function parseItemKey<K extends string>(
  key: string | null,
  kinds: readonly K[],
): Selection<K> | null {
  if (!key) return null;
  const separator = key.indexOf(":");
  if (separator === -1) return null;
  const kind = key.slice(0, separator);
  const id = key.slice(separator + 1);
  if (kind !== "parked" && !kinds.some((candidate) => candidate === kind)) return null;
  return { kind: kind as K | "parked", id };
}
