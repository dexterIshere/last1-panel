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
  const contrast = (channel.contrast - 50) / 50 / 3;
  const brightness = (channel.brightness - 50) / 50 / 3;
  const slope = contrast > 0 ? 0.5 / (0.5 - contrast) : (0.5 + contrast) / 0.5;
  const y = Math.min(1, Math.max(0, (x - 0.5) * slope + 0.5 + brightness));
  return Math.pow(y, 1 / channel.gamma);
}

export const isNeutral = (c: Channel): boolean => c.brightness === 50 && c.contrast === 50 && c.gamma === 1;

export const sameChannels = (a: ColorAdjust): boolean =>
  JSON.stringify(a.red) === JSON.stringify(a.green) && JSON.stringify(a.red) === JSON.stringify(a.blue);
