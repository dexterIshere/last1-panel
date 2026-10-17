// Langue de l'interface : celle du système (system_info), ou celle choisie
// dans la barre latérale, gardée pour les lancements suivants. Français pour
// un système en français, anglais sinon (comme last1.fun et go-to-linux).

export type Lang = "fr" | "en";

const STORAGE_KEY = "last1-panel-lang";

class I18n {
  lang = $state<Lang>("fr");
}

export const i18n = new I18n();

/** Le texte dans la langue de l'interface. */
export const t = (fr: string, en: string): string => (i18n.lang === "en" ? en : fr);

/** Langue des nombres (Intl). */
export const locale = (): string => (i18n.lang === "en" ? "en-US" : "fr-FR");

export const isLang = (value: unknown): value is Lang => value === "fr" || value === "en";

export function savedLang(): Lang | null {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return isLang(value) ? value : null;
  } catch {
    return null;
  }
}

export function saveLang(lang: Lang): void {
  try {
    localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    // Préférence non gardée : la langue du système reviendra au prochain lancement.
  }
}

/** Nombre dans la langue de l'interface (1,25 ou 1.25). */
export function num(value: number, digits = 0): string {
  return value.toLocaleString(locale(), { minimumFractionDigits: digits, maximumFractionDigits: digits });
}
