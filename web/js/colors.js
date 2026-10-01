// Route colours: one hue per selected kadı, lightness ramps from the first
// appointment (light) to the last (dark).

const HUES = [212, 4, 146, 274, 30, 188, 328, 86];

export const MAX_ROUTES = HUES.length;

export function pickHue(usedHues = []) {
  return HUES.find((hue) => !usedHues.includes(hue)) ?? HUES[usedHues.length % HUES.length];
}

// t: 0 = first step, 1 = last step.
export function routeColor(hue, t) {
  const dark = document.documentElement.dataset.theme === "dark";
  const [from, to] = dark ? [84, 48] : [76, 27];
  const saturation = dark ? 72 : 66;
  const lightness = from + (to - from) * clamp01(t);

  return `hsl(${hue} ${saturation}% ${lightness.toFixed(1)}%)`;
}

export function routeGradient(hue) {
  return `linear-gradient(90deg, ${routeColor(hue, 0)}, ${routeColor(hue, 1)})`;
}

// Positions along a route of `count` steps: step i leaves from i/count and
// arrives at (i+1)/count, so the first place is the lightest and the last the
// darkest even for a one-step route. The step itself sits halfway.
export function departurePosition(index, count) {
  return count > 0 ? index / count : 0;
}

export function arrivalPosition(index, count) {
  return count > 0 ? (index + 1) / count : 1;
}

export function stepPosition(index, count) {
  return count > 0 ? (index + 0.5) / count : 0.5;
}

// Text colour that stays readable on top of routeColor(hue, t).
export function onRouteColor(t) {
  const dark = document.documentElement.dataset.theme === "dark";
  if (dark) return "#071521";
  return t < 0.45 ? "#18222d" : "#ffffff";
}

function clamp01(value) {
  return Math.min(1, Math.max(0, Number(value) || 0));
}
