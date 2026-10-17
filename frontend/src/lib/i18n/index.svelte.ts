import { esAR, en, type LocaleFormat } from '../locale';
import es from './es';
import enDict from './en';
import type { Dict, Lang } from './types';

export type { Dict, Lang };

/**
 * Everything one language needs to render a screen: its dictionary and its
 * formatters.
 *
 * Kept as one object so a component cannot take the words from one language and
 * the money from another — the pair moves together or not at all.
 */
export interface Locale {
  readonly tag: Lang;
  readonly dict: Dict;
  readonly fmt: LocaleFormat;
}

export const locales: Record<Lang, Locale> = {
  'es-AR': { tag: 'es-AR', dict: es, fmt: esAR },
  en: { tag: 'en', dict: enDict, fmt: en }
};

/**
 * The active language.
 *
 * `es-AR` is the default and the choice is deliberately not persisted (S2): the
 * shop opens in its own language every time, and the toggle exists so a
 * developer can read the English side without editing code. A reload returning
 * to es-AR is the intended behaviour, not a missing feature.
 */
class LangState {
  current = $state<Lang>('es-AR');

  get locale(): Locale {
    return locales[this.current];
  }

  /** The dictionary, at the call site components already use. */
  get t(): Dict {
    return this.locale.dict;
  }

  /** The formatters for money, quantities and dates. */
  get fmt(): LocaleFormat {
    return this.locale.fmt;
  }

  /** The label of the *other* language, which is what the toggle offers. */
  get other(): string {
    return this.current === 'es-AR' ? 'EN' : 'ES';
  }

  toggle() {
    this.current = this.current === 'es-AR' ? 'en' : 'es-AR';
  }
}

export const lang = new LangState();
