export function skillMarkdownPath(folderPath: string): string {
  return `${folderPath.replace(/\/+$/, "")}/SKILL.md`;
}
