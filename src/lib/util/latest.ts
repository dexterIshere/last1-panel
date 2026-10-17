// Envoi du dernier réglage seulement : pendant qu'un curseur bouge, les
// valeurs intermédiaires sont regroupées (délai `delay`), et un seul envoi
// est en cours à la fois. Le dernier réglage demandé est toujours envoyé.

export interface Latest<T> {
  push(value: T): void;
  /** Envoie tout de suite (relâchement du curseur, bouton). */
  flush(): Promise<void>;
  readonly busy: boolean;
}

export function latest<T>(send: (value: T) => Promise<void>, delay: number, onError: (error: unknown) => void): Latest<T> {
  let pending: { value: T } | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let running: Promise<void> | null = null;

  async function drain(): Promise<void> {
    while (pending) {
      const { value } = pending;
      pending = null;
      try {
        await send(value);
      } catch (error) {
        onError(error);
      }
    }
  }

  function start(): Promise<void> {
    clearTimeout(timer);
    if (!running) {
      running = drain().finally(() => (running = null));
    }
    return running;
  }

  return {
    push(value: T) {
      pending = { value };
      clearTimeout(timer);
      timer = setTimeout(start, delay);
    },
    async flush() {
      await start();
      // Une valeur arrivée pendant l'envoi : envoyée à son tour.
      if (pending) await start();
    },
    get busy() {
      return running !== null;
    },
  };
}
