import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { apcaContrast, isInSrgbGamut, oklchToHex, parseOklch } from "../src/color.ts";

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const tokensPath = join(packageRoot, "src", "tokens.css");
const declarationsPath = join(packageRoot, "CONTRAST.md");

function parseModeBlock(css, selector) {
  const start = css.indexOf(`${selector} {`);
  if (start === -1) throw new Error(`tokens.css has no "${selector}" block`);
  const end = css.indexOf("\n}", start);
  if (end === -1) throw new Error(`the "${selector}" block in tokens.css is not closed`);
  const body = css.slice(start, end);
  const tokens = new Map();
  for (const [, name, value] of body.matchAll(/--color-([a-z0-9-]+):\s*([^;]+);/g)) {
    const color = parseOklch(value);
    if (!color) throw new Error(`--color-${name} in "${selector}" is not an oklch() value: ${value}`);
    tokens.set(name, color);
  }
  if (tokens.size === 0) throw new Error(`the "${selector}" block declares no colour tokens`);
  return tokens;
}

function parseDeclaredPairs(markdown) {
  const pairs = [];
  for (const rawLine of markdown.split("\n")) {
    const line = rawLine.trim().replace(/^[-*]\s+/, "").replace(/`/g, "");
    const match =
      /^([a-z0-9-]+)\s+on\s+([a-z0-9-]+):\s*([\d.]+)(?:\s+max\s+([\d.]+))?(?:\s+\(dark (?:min\s+([\d.]+)|step\s+([\d.]+)\.\.([\d.]+))\))?$/.exec(
        line,
      );
    if (match) {
      pairs.push({
        foreground: match[1],
        background: match[2],
        threshold: Number(match[3]),
        ceiling: match[4] === undefined ? Infinity : Number(match[4]),
        darkThreshold: match[5] === undefined ? undefined : Number(match[5]),
        darkStep:
          match[6] === undefined ? undefined : { floor: Number(match[6]), ceiling: Number(match[7]) },
      });
    }
  }
  if (pairs.length === 0) throw new Error("CONTRAST.md declares no pairs");
  return pairs;
}

function parseSectionLevels(markdown, sectionHeading) {
  const start = markdown.indexOf(sectionHeading);
  const byTheme = new Map();
  if (start === -1) return byTheme;
  let current = null;
  for (const rawLine of markdown.slice(start + sectionHeading.length).split("\n")) {
    const line = rawLine.trim().replace(/`/g, "");
    if (line.startsWith("## ")) break;
    const themeHeading = /^###\s+([a-z0-9-]+)\s+(light|dark)$/.exec(line);
    if (themeHeading) {
      current = `${themeHeading[1]} ${themeHeading[2]}`;
      byTheme.set(current, new Map());
      continue;
    }
    const match = /^([a-z0-9-]+)\s+on\s+([a-z0-9-]+):\s*([\d.]+)$/.exec(line);
    if (match && current) byTheme.get(current).set(`${match[1]} on ${match[2]}`, Number(match[3]));
  }
  return byTheme;
}

function parseDeviations(markdown) {
  return parseSectionLevels(markdown, "## Declared deviations");
}

const THEMES = [
  { id: "default", light: ":root", dark: ".dark" },
  {
    id: "high-contrast",
    light: '[data-palette="high-contrast"]',
    dark: '[data-palette="high-contrast"].dark',
    waiveCeilings: true,
    textFloor: 90,
  },
  {
    id: "dracula",
    light: '[data-palette="dracula"]',
    dark: '[data-palette="dracula"].dark',
    cosmetic: true,
  },
];

const DEVIATION_TOLERANCE = 0.5;

// The level at and above which a declared pair is a reading task. A theme with a `textFloor` holds
// every one of those to that floor instead of the pair's own, which is what makes "high contrast"
// mean higher contrast than the default theme rather than merely passing the same bar.
const TEXT_LEVEL = 60;

// APCA clamps low contrast to exactly Lc 0.0, which a dark hairline reaches while still being
// plainly visible. Pairs that hit that floor are gated on the rendered 8-bit step instead.
function renderedStep(a, b) {
  const level = (c) => parseInt(oklchToHex(c).slice(1, 3), 16);
  return Math.abs(level(a) - level(b));
}

const tokensCss = readFileSync(tokensPath, "utf8");
const declarations = readFileSync(declarationsPath, "utf8");
const appendixStart = ["## Declared deviations", "## The high contrast floor"]
  .map((heading) => declarations.indexOf(heading))
  .filter((index) => index !== -1)
  .reduce((a, b) => Math.min(a, b), declarations.length);
const pairs = parseDeclaredPairs(declarations.slice(0, appendixStart));
const deviations = parseDeviations(declarations);
const exemptions = parseSectionLevels(declarations, "## The high contrast floor");

const baseLight = parseModeBlock(tokensCss, ":root");
const darkOverrides = parseModeBlock(tokensCss, ".dark");
const baseDark = new Map([...baseLight, ...darkOverrides]);

// App icon artwork that must render identically in every theme and mode, exempt from the
// light-has-dark gate and forbidden from carrying a palette override.
const THEME_INVARIANT = new Set(["brand", "brand-contrast"]);

const failures = [];
const lightNames = [...baseLight.keys()];
const darkOverrideNames = [...darkOverrides.keys()];

for (const name of darkOverrideNames) {
  if (!lightNames.includes(name)) failures.push(`--color-${name} has no light-mode value`);
}

for (const name of lightNames) {
  if (!THEME_INVARIANT.has(name) && !darkOverrideNames.includes(name)) {
    failures.push(`--color-${name} has no dark-mode value`);
  }
}

for (const name of THEME_INVARIANT) {
  if (darkOverrideNames.includes(name)) {
    failures.push(`--color-${name} is declared theme-invariant but has a dark-mode override`);
  }
}

// A palette block must declare every token in both of its modes. Partial blocks would resolve by
// source order against `.dark`, which is the one way this file can render a colour the gate never
// measured.
const paletteBlocks = new Map();
for (const theme of THEMES) {
  if (theme.id === "default") continue;
  const light = parseModeBlock(tokensCss, theme.light);
  const dark = parseModeBlock(tokensCss, theme.dark);
  paletteBlocks.set(theme.id, { light, dark });
  for (const name of lightNames) {
    if (THEME_INVARIANT.has(name)) {
      if (light.has(name) || dark.has(name)) {
        failures.push(`${theme.id}: --color-${name} is theme-invariant and cannot be overridden`);
      }
      continue;
    }
    if (!light.has(name)) failures.push(`${theme.id}: --color-${name} missing from its light block`);
    if (!dark.has(name)) failures.push(`${theme.id}: --color-${name} missing from its dark block`);
  }
}

function resolve(theme, mode) {
  if (theme.id === "default") return mode === "light" ? baseLight : baseDark;
  const { light, dark } = paletteBlocks.get(theme.id);
  return mode === "light"
    ? new Map([...baseLight, ...light])
    : new Map([...baseDark, ...light, ...dark]);
}

const runs = [];
for (const theme of THEMES) {
  for (const mode of ["light", "dark"]) runs.push({ theme, mode, tokens: resolve(theme, mode) });
}

for (const { theme, mode, tokens } of runs) {
  for (const [name, color] of tokens) {
    if (!isInSrgbGamut(color)) {
      failures.push(`${theme.id} ${mode}: --color-${name} is outside the sRGB gamut`);
    }
  }
}

let checked = 0;
for (const { theme, mode, tokens } of runs) {
  console.log(`\n${theme.id} ${mode}`);
  const declared = deviations.get(`${theme.id} ${mode}`) ?? new Map();
  const exempt = exemptions.get(`${theme.id} ${mode}`) ?? new Map();
  const seen = new Set();
  const exemptSeen = new Set();
  for (const { foreground, background, threshold, ceiling, darkThreshold, darkStep } of pairs) {
    checked += 1;
    const label = `${foreground} on ${background}`;
    const fg = tokens.get(foreground);
    const bg = tokens.get(background);
    if (!fg || !bg) {
      failures.push(`${theme.id} ${mode}: ${label} references an undeclared token`);
      console.log(`  MISSING  ${label}`);
      continue;
    }
    if (fg.alpha !== 1 || bg.alpha !== 1) {
      failures.push(`${theme.id} ${mode}: ${label} uses a translucent token`);
      console.log(`  ALPHA    ${label}`);
      continue;
    }
    const onStep = mode === "dark" && darkStep !== undefined;
    const unit = onStep ? "step" : "Lc";
    const measured = onStep ? renderedStep(fg, bg) : Math.abs(apcaContrast(fg, bg));
    const effectiveThreshold = onStep
      ? darkStep.floor
      : mode === "dark" && darkThreshold !== undefined
        ? darkThreshold
        : threshold;
    const declaredCeiling = onStep ? darkStep.ceiling : ceiling;
    const effectiveCeiling = theme.waiveCeilings ? Infinity : declaredCeiling;
    const raised = theme.textFloor !== undefined && !onStep && threshold >= TEXT_LEVEL;
    const exemption = raised ? exempt.get(label) : undefined;
    if (exemption !== undefined) exemptSeen.add(label);
    const floor =
      raised && exemption === undefined
        ? Math.max(effectiveThreshold, theme.textFloor)
        : effectiveThreshold;
    const under = measured + 1e-9 < floor;
    const over = measured - 1e-9 > effectiveCeiling;
    const allowance = declared.get(label);
    if (allowance !== undefined) seen.add(label);

    let verdict = "pass";
    if (exemption !== undefined && measured + 1e-9 >= theme.textFloor) {
      failures.push(
        `${theme.id} ${mode}: ${label} is exempt from the ${theme.textFloor} floor but reaches ${unit} ${measured.toFixed(1)}; remove it from the contract`,
      );
      verdict = "STALE";
    } else if (exemption !== undefined && Math.abs(measured - exemption) > DEVIATION_TOLERANCE) {
      failures.push(
        `${theme.id} ${mode}: ${label} is exempt at ${unit} ${exemption} but measures ${unit} ${measured.toFixed(1)}`,
      );
      verdict = "STALE";
    } else if (under && theme.cosmetic && allowance !== undefined) {
      if (Math.abs(measured - allowance) > DEVIATION_TOLERANCE) {
        failures.push(
          `${theme.id} ${mode}: ${label} is declared as a deviation at ${unit} ${allowance} but measures ${unit} ${measured.toFixed(1)}`,
        );
        verdict = "STALE";
      } else {
        verdict = "dev";
      }
    } else if (under) {
      failures.push(`${theme.id} ${mode}: ${label} is ${unit} ${measured.toFixed(1)}, below ${unit} ${floor}`);
      verdict = "FAIL";
    } else if (over) {
      failures.push(
        `${theme.id} ${mode}: ${label} is ${unit} ${measured.toFixed(1)}, above the ${unit} ${effectiveCeiling} ceiling`,
      );
      verdict = "FAIL";
    } else if (allowance !== undefined) {
      failures.push(
        `${theme.id} ${mode}: ${label} is declared as a deviation but clears ${unit} ${effectiveThreshold} at ${unit} ${measured.toFixed(1)}; remove it from the contract`,
      );
      verdict = "STALE";
    }

    const bounds =
      effectiveCeiling === Infinity ? `(min ${floor})` : `(min ${floor}, max ${effectiveCeiling})`;
    console.log(
      `  ${verdict.padEnd(8)} ${label.padEnd(36)}${unit.padStart(4)} ${measured.toFixed(1).padStart(5)}  ${bounds}`,
    );
  }
  for (const label of declared.keys()) {
    if (!seen.has(label)) {
      failures.push(`${theme.id} ${mode}: declared deviation ${label} is not a declared pair`);
    }
  }
  for (const label of exempt.keys()) {
    if (!exemptSeen.has(label)) {
      failures.push(`${theme.id} ${mode}: ${label} is exempt from the floor but is not a text pair`);
    }
  }
}

console.log(`\n${pairs.length} pairs x ${runs.length} theme/mode combinations = ${checked} checks`);
if (failures.length > 0) {
  console.error(`\n${failures.length} contrast failure(s):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}
console.log("every declared pair clears its declared level in every theme and mode");
