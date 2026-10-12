export type Oklch = { l: number; c: number; h: number; alpha?: number };

const OKLAB_TO_LMS = [
  [1, 0.3963377773761749, 0.2158037573099136],
  [1, -0.1055613458156586, -0.0638541728258133],
  [1, -0.0894841775298119, -1.2914855480194092],
];
const LMS_TO_LINEAR_RGB = [
  [4.0767416621, -3.3077115913, 0.2309699292],
  [-1.2684380046, 2.6097574011, -0.3413193965],
  [-0.0041960863, -0.7034186147, 1.707614701],
];
const LINEAR_RGB_TO_LMS = [
  [0.4122214708, 0.5363325363, 0.0514459929],
  [0.2119034982, 0.6806995451, 0.1073969566],
  [0.0883024619, 0.2817188376, 0.6299787005],
];
const LMS_TO_OKLAB = [
  [0.2104542553, 0.793617785, -0.0040720468],
  [1.9779984951, -2.428592205, 0.4505937099],
  [0.0259040371, 0.7827717662, -0.808675766],
];

export function oklchToLinearRgb({ l, c, h }: Oklch): number[] {
  const radians = (h * Math.PI) / 180;
  const lab = [l, c * Math.cos(radians), c * Math.sin(radians)];
  const lms = OKLAB_TO_LMS.map((row) => row[0] * lab[0] + row[1] * lab[1] + row[2] * lab[2]);
  const cubed = lms.map((v) => v * v * v);
  return LMS_TO_LINEAR_RGB.map((row) => row[0] * cubed[0] + row[1] * cubed[1] + row[2] * cubed[2]);
}

export function encodeChannel(value: number): number {
  return value <= 0.0031308 ? 12.92 * value : 1.055 * Math.pow(value, 1 / 2.4) - 0.055;
}

export function decodeChannel(value: number): number {
  return value <= 0.04045 ? value / 12.92 : Math.pow((value + 0.055) / 1.055, 2.4);
}

export function isInSrgbGamut(color: Oklch): boolean {
  return oklchToLinearRgb(color)
    .map(encodeChannel)
    .every((v) => v >= -0.0005 && v <= 1.0005);
}

/** APCA, W3 0.1.9 constants. */
const APCA = {
  trc: 2.4,
  coefficients: [0.2126729, 0.7151522, 0.072175],
  normBg: 0.56,
  normText: 0.57,
  revText: 0.62,
  revBg: 0.65,
  blackThreshold: 0.022,
  blackClamp: 1.414,
  scale: 1.14,
  offset: 0.027,
  loClip: 0.1,
  deltaYMin: 0.0005,
};

function apcaLuminance(color: Oklch): number {
  const encoded = oklchToLinearRgb(color).map((v) => Math.min(1, Math.max(0, encodeChannel(v))));
  return encoded.reduce((sum, channel, i) => sum + APCA.coefficients[i] * Math.pow(channel, APCA.trc), 0);
}

function clampBlack(y: number): number {
  return y > APCA.blackThreshold ? y : y + Math.pow(APCA.blackThreshold - y, APCA.blackClamp);
}

/** Polarity-aware: negative means light text on a dark ground. Use {@link apcaLc} for a magnitude. */
export function apcaContrast(foreground: Oklch, background: Oklch): number {
  const textY = clampBlack(apcaLuminance(foreground));
  const bgY = clampBlack(apcaLuminance(background));
  if (Math.abs(bgY - textY) < APCA.deltaYMin) return 0;
  if (bgY > textY) {
    const sapc = (Math.pow(bgY, APCA.normBg) - Math.pow(textY, APCA.normText)) * APCA.scale;
    return sapc < APCA.loClip ? 0 : (sapc - APCA.offset) * 100;
  }
  const sapc = (Math.pow(bgY, APCA.revBg) - Math.pow(textY, APCA.revText)) * APCA.scale;
  return sapc > -APCA.loClip ? 0 : (sapc + APCA.offset) * 100;
}

export function apcaLc(foreground: Oklch, background: Oklch): number {
  return Math.abs(apcaContrast(foreground, background));
}

export function parseOklch(value: string): Oklch | null {
  const match = /^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*(?:\/\s*([\d.]+)\s*)?\)$/.exec(value.trim());
  if (!match) return null;
  return {
    l: Number(match[1]),
    c: Number(match[2]),
    h: Number(match[3]),
    alpha: match[4] === undefined ? 1 : Number(match[4]),
  };
}

export function formatOklch({ l, c, h, alpha }: Oklch): string {
  const body = `${round(l, 4)} ${round(c, 4)} ${round(h, 2)}`;
  return alpha === undefined || alpha === 1 ? `oklch(${body})` : `oklch(${body} / ${alpha})`;
}

function round(value: number, places: number): number {
  return Number(value.toFixed(places));
}

const HEX = /^#([0-9a-f]{6})$/i;

export function hexToOklch(hex: string): Oklch | null {
  const match = HEX.exec(hex.trim());
  if (!match) return null;
  const rgb = [0, 2, 4].map((i) => decodeChannel(parseInt(match[1].slice(i, i + 2), 16) / 255));
  const lms = LINEAR_RGB_TO_LMS.map((row) =>
    Math.cbrt(row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2]),
  );
  const lab = LMS_TO_OKLAB.map((row) => row[0] * lms[0] + row[1] * lms[1] + row[2] * lms[2]);
  let h = (Math.atan2(lab[2], lab[1]) * 180) / Math.PI;
  if (h < 0) h += 360;
  return { l: lab[0], c: Math.hypot(lab[1], lab[2]), h };
}

export function oklchToHex(color: Oklch): string {
  return (
    "#" +
    oklchToLinearRgb(color)
      .map((v) =>
        Math.round(Math.min(1, Math.max(0, encodeChannel(v))) * 255)
          .toString(16)
          .padStart(2, "0")
          .toUpperCase(),
      )
      .join("")
  );
}

export function maxChroma(l: number, h: number): number {
  let lo = 0;
  let hi = 0.4;
  for (let i = 0; i < 40; i++) {
    const mid = (lo + hi) / 2;
    if (isInSrgbGamut({ l, c: mid, h })) lo = mid;
    else hi = mid;
  }
  return lo;
}

export function clampToGamut(color: Oklch): Oklch {
  if (isInSrgbGamut(color)) return color;
  return { ...color, c: maxChroma(color.l, color.h) };
}

/**
 * Move `color` along lightness, away from `ground`, until the pair reaches `target`. Returns the
 * closest reachable colour when the target is out of range at this hue, so a caller always gets a
 * usable value rather than a failure.
 */
export function solveForLc(color: Oklch, ground: Oklch, target: number): Oklch {
  if (apcaLc(color, ground) >= target) return color;
  const darker = ground.l > color.l;
  let best = color;
  let bestLc = apcaLc(color, ground);
  for (let step = 1; step <= 100; step++) {
    const l = color.l + (darker ? -step : step) / 100;
    if (l < 0 || l > 1) break;
    const candidate = clampToGamut({ ...color, l });
    const lc = apcaLc(candidate, ground);
    if (lc > bestLc) {
      best = candidate;
      bestLc = lc;
    }
    if (lc >= target) return candidate;
  }
  return best;
}
