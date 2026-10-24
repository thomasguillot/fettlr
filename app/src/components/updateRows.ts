export type UpdateRow = {
  id: string;
  kind: "marketplace" | "plugin" | "skill" | "mcpServer" | "accountSkill";
  name: string;
  note: string | null;
  detail: string;
  action: ((id: string) => void) | null;
  actionKey: string;
  inUpdateAll: boolean;
};

export function updateAllCount(rows: readonly UpdateRow[]): number {
  return rows.filter((row) => row.inUpdateAll && row.action !== null).length;
}
