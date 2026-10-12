import { writeFile } from "node:fs/promises";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "astro/config";
import { SITE_URL, bootstrapSkillUrl } from "./src/site.ts";

export default defineConfig({
  site: SITE_URL,
  integrations: [
    {
      name: "fettlr-install-proxy",
      hooks: {
        "astro:build:done": async ({ dir }) => {
          await writeFile(new URL("_redirects", dir), `/install  ${bootstrapSkillUrl()}  200\n`);
        },
      },
    },
  ],
  vite: {
    plugins: [tailwindcss()],
    build: {
      cssMinify: "esbuild",
    },
  },
});
