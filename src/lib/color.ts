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

/** Moyenne des trois canaux, au pas des curseurs : ce que montre « Tous ». */
export const meanChannel = (a: ColorAdjust): Channel => ({
  brightness: clampChannel("brightness", (a.red.brightness + a.green.brightness + a.blue.brightness) / 3),
  contrast: clampChannel("contrast", (a.red.contrast + a.green.contrast + a.blue.contrast) / 3),
  gamma: clampChannel("gamma", (a.red.gamma + a.green.gamma + a.blue.gamma) / 3),
});

export const isNeutral = (c: Channel): boolean => c.brightness === 50 && c.contrast === 50 && c.gamma === 1;

export const sameChannels = (a: ColorAdjust): boolean =>
  JSON.stringify(a.red) === JSON.stringify(a.green) && JSON.stringify(a.red) === JSON.stringify(a.blue);
