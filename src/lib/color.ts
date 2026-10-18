// Courbes de couleur, comme core/src/color/curve.rs : sert à dessiner le
// graphe et l'aperçu (les deux calculs doivent rester identiques).

import type { Channel, ColorAdjust, OwnChannels } from "$lib/api/types";

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

export type ChannelName = (typeof CHANNELS)[number];
const KEYS = ["brightness", "contrast", "gamma"] as const;
const isDefault = (key: keyof Channel, value: number) => Math.abs(value - DEFAULT_CHANNEL[key]) < (key === "gamma" ? 0.0005 : 0.05);

/** Curseurs de « Tous » et de chaque canal, comme le panneau NVIDIA : un
 * curseur de canal laissé à sa valeur par défaut suit « Tous ». */
export interface Sliders {
  all: Channel;
  own: OwnChannels;
}

/** Copie simple : les objets réactifs de Svelte ne passent pas par structuredClone. */
const copyOwn = (o: OwnChannels): OwnChannels => ({ red: { ...o.red }, green: { ...o.green }, blue: { ...o.blue } });

/** Curseurs gardés avec le réglage ; pour un réglage d'avant, « Tous » est
 * ce qu'au moins deux canaux partagent (sinon le rouge) et chaque canal
 * garde ce qui en diffère. */
export function sliders(a: ColorAdjust): Sliders {
  const all = a.all ? { ...a.all } : sameChannel(a.green, a.blue) ? { ...a.green } : { ...a.red };
  if (a.own) return { all, own: copyOwn(a.own) };
  const own = (c: Channel): Channel => ({
    brightness: Math.abs(c.brightness - all.brightness) < 0.05 ? DEFAULT_CHANNEL.brightness : c.brightness,
    contrast: Math.abs(c.contrast - all.contrast) < 0.05 ? DEFAULT_CHANNEL.contrast : c.contrast,
    gamma: Math.abs(c.gamma - all.gamma) < 0.0005 ? DEFAULT_CHANNEL.gamma : c.gamma,
  });
  return { all, own: { red: own(a.red), green: own(a.green), blue: own(a.blue) } };
}

/** Les courbes de l'écran d'après les curseurs : chaque canal prend ses
 * valeurs propres, et celles de « Tous » ailleurs. */
export function resolve({ all, own }: Sliders): ColorAdjust {
  const channel = (c: Channel): Channel =>
    Object.fromEntries(KEYS.map((key) => [key, isDefault(key, c[key]) ? all[key] : c[key]])) as unknown as Channel;
  return { red: channel(own.red), green: channel(own.green), blue: channel(own.blue), all: { ...all }, own: copyOwn(own) };
}

/** Canaux qui ont au moins un curseur propre. */
export const ownChannels = (s: Sliders): ChannelName[] => CHANNELS.filter((c) => KEYS.some((key) => !isDefault(key, s.own[c][key])));

export const isNeutral = (c: Channel): boolean => c.brightness === 50 && c.contrast === 50 && c.gamma === 1;

export const sameChannels = (a: ColorAdjust): boolean =>
  JSON.stringify(a.red) === JSON.stringify(a.green) && JSON.stringify(a.red) === JSON.stringify(a.blue);
