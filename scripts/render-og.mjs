// Run against a dev server with the globally installed Playwright:
//   NODE_PATH=$(npm root -g)/@playwright/cli/node_modules node scripts/render-og.mjs
import { chromium } from "playwright";

const url = process.argv[2] ?? "http://localhost:4321/";
const out = process.argv[3] ?? "site/public/og.png";

const WIDTH = 1200;
const HEIGHT = 630;

// The card is a recomposition of the hero, not a plain viewport capture: the wordmark row, the lead,
// spec line and the call to action come out, and the two constants below place what is left so the
// headline sits above the top of the app window. Changing either reframes the card.
const HERO_TO_WINDOW = 44;
const SCROLL = 44;

const compose = `
  astro-dev-toolbar { display: none !important; }
  main > section:first-of-type .n-grid > *:nth-child(1),
  main > section:first-of-type .n-grid > *:nth-child(3),
  main > section:first-of-type .n-grid > *:nth-child(4),
  main > section:first-of-type .n-grid > *:nth-child(5) { display: none !important; }
  .n-attached { margin-top: ${HERO_TO_WINDOW}px !important; }
`;

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: WIDTH, height: HEIGHT },
  deviceScaleFactor: 1,
  reducedMotion: "reduce",
  colorScheme: "light",
});

const response = await page.goto(url, { waitUntil: "networkidle" });
if (!response || !response.ok()) {
  console.error(`could not load ${url} (${response ? response.status() : "no response"})`);
  await browser.close();
  process.exit(1);
}
await page.addStyleTag({ content: compose });
await page.evaluate(() => document.fonts.ready);
await page.evaluate((y) => window.scrollTo(0, y), SCROLL);
await page.waitForTimeout(400);
await page.screenshot({ path: out });
await browser.close();

console.log(`wrote ${out} (${WIDTH}x${HEIGHT}) from ${url}`);
