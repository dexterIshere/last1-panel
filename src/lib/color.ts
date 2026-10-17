// Courbes de couleur, comme core/src/color/curve.rs : sert à dessiner le
// graphe et l'aperçu (les deux calculs doivent rester identiques).

import type { Channel, ColorAdjust } from "$lib/api/types";

export const DEFAULT_CHANNEL: Channel = { brightness: 50, contrast: 50, gamma: 1 };

export const neutralAdjust = (): ColorAdjust => ({
  red: { ...DEFAULT_CHANNEL },
  green: { ...DEFAULT_CHANNEL },
  blue: { ...DEFAULT_CHANNEL },
});

export function applyChannel(channel: Channel, x: number): number {
  const contrast = (channel.contrast - 50) / 50;
  const brightness = (channel.brightness - 50) / 50;
  let offset = contrast > 0 ? (-contrast * 25.4) / 255 : (-contrast * 32) / 255;
  const range = 1 + 2 * offset;
  offset += (brightness * range) / 5;
  const y = (x + offset) / range;
  return y <= 0 ? 0 : Math.min(1, Math.pow(y, 1 / channel.gamma));
}

export const CHANNELS = ["red", "green", "blue"] as const;

const BOUNDS: Record<keyof Channel, [number, number, number]> = {
  brightness: [0, 100, 1],
  contrast: [0, 100, 1],
  gamma: [0.3, 2.8, 0.01],
};

/** Valeur ramenée dans les bornes du panneau NVIDIA, au pas du curseur. */
export function clampChannel(key: keyof Channel, value: number): number {
  const [min, max, step] = BOUNDS[key];
  return Math.round(Math.min(max, Math.max(min, value)) / step) * step;
}

/** Même réglage, aux arrondis des curseurs près. */
export const sameChannel = (a: Channel, b: Channel): boolean =>
  Math.abs(a.brightness - b.brightness) < 0.05 && Math.abs(a.contrast - b.contrast) < 0.05 && Math.abs(a.gamma - b.gamma) < 0.0005;

/** Valeurs de « Tous » : celles gardées avec le réglage ; pour un réglage
 * d'avant, celles qu'au moins deux canaux partagent, sinon celles du rouge. */
export function allChannel(a: ColorAdjust): Channel {
  if (a.all) return { ...a.all };
  return sameChannel(a.green, a.blue) ? { ...a.green } : { ...a.red };
}

/** Canaux réglés à part : « Tous » ne les change pas. */
export const ownChannels = (a: ColorAdjust): (typeof CHANNELS)[number][] => {
  const all = allChannel(a);
  return CHANNELS.filter((c) => !sameChannel(a[c], all));
};

export const isNeutral = (c: Channel): boolean => c.brightness === 50 && c.contrast === 50 && c.gamma === 1;

export const sameChannels = (a: ColorAdjust): boolean =>
  JSON.stringify(a.red) === JSON.stringify(a.green) && JSON.stringify(a.red) === JSON.stringify(a.blue);
