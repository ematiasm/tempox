import { beforeEach, describe, expect, it } from 'vitest';
import {
  formatDay,
  formatMoney,
  formatQuantity,
  formatTime,
  lineTotalAmount,
  parseAmount,
  parseInitialStock,
  parseQuantity,
  sumAmounts
} from '#lib/money.js';
import { lang } from '#lib/i18n/index.svelte.js';

/**
 * The arithmetic, and the locale boundary.
 *
 * The functions here are the ones every screen calls. Two things are pinned:
 *
 * 1. The arithmetic is language-independent and unchanged (`lineTotalAmount`,
 *    `sumAmounts`, `parseQuantity`). Those tests predate the locale and must
 *    keep passing verbatim — that is the preserved-behavior half of S17.
 * 2. The formatting delegates to the active locale, and the default is es-AR.
 *    The exact strings per language live in `locale.test.ts`; here it is only
 *    proved that this file follows the language that is active.
 */
beforeEach(() => {
  // The toggle is module state; each test starts from the shop's own language.
  lang.current = 'es-AR';
});

describe('formatMoney follows the active language', () => {
  it('formats in es-AR by default, which is the shop language', () => {
    expect(lang.current).toBe('es-AR');
    expect(formatMoney('25.00')).toBe('$ 25,00');
    expect(formatMoney('184300.00')).toBe('$ 184.300,00');
    expect(formatMoney('-7200.00')).toBe('-$ 7.200,00');
  });

  it('formats the same amount as before when English is selected', () => {
    // The e2e suite asserted these exact strings before the locale landed; the
    // English side is not allowed to drift because Spanish arrived.
    lang.current = 'en';
    expect(formatMoney('25.00')).toBe('$25.00');
    expect(formatMoney('0.01')).toBe('$0.01');
    expect(formatMoney('0.00')).toBe('$0.00');
    expect(formatMoney('184300.00')).toBe('$184,300.00');
    expect(formatMoney('-7200.00')).toBe('-$7,200.00');
  });

  it('switches on the toggle, in both directions', () => {
    expect(formatMoney('1234.56')).toBe('$ 1.234,56');
    lang.toggle();
    expect(lang.current).toBe('en');
    expect(formatMoney('1234.56')).toBe('$1,234.56');
    lang.toggle();
    expect(lang.current).toBe('es-AR');
    expect(formatMoney('1234.56')).toBe('$ 1.234,56');
  });
});

describe('parseAmount follows the active language', () => {
  it('reads the es-AR forms by default', () => {
    expect(parseAmount('25')).toBe('25.00');
    expect(parseAmount('25,5')).toBe('25.50');
    expect(parseAmount('1.234,56')).toBe('1234.56');
    expect(parseAmount('0,01')).toBe('0.01');
    expect(parseAmount(' 25,00 ')).toBe('25.00');
    expect(parseAmount('0,00')).toBe('0.00');
  });

  it('reads the en forms when English is selected', () => {
    lang.current = 'en';
    expect(parseAmount('25')).toBe('25.00');
    expect(parseAmount('25.00')).toBe('25.00');
    expect(parseAmount('25.5')).toBe('25.50');
    expect(parseAmount('1,234.56')).toBe('1234.56');
  });

  it('rejects sub-cent amounts under the API contract, not a display rule', () => {
    expect(parseAmount('0,005')).toBeNull();
    expect(parseAmount('1,999')).toBeNull();
    lang.current = 'en';
    expect(parseAmount('0.005')).toBeNull();
    expect(parseAmount('1.999')).toBeNull();
  });

  it('rejects text that is not an amount, in both languages', () => {
    for (const text of ['', '   ', 'abc', '-5', '1e3']) {
      expect(parseAmount(text)).toBeNull();
      lang.current = lang.current === 'en' ? 'es-AR' : 'en';
      expect(parseAmount(text)).toBeNull();
    }
  });

  it('round-trips amount strings through display formatting, per language', () => {
    for (const amount of ['0.00', '0.01', '0.99', '1.00', '25.00', '1234.56', '184300.00']) {
      expect(parseAmount(formatMoney(amount))).toBe(amount);
      lang.current = 'en';
      expect(parseAmount(formatMoney(amount))).toBe(amount);
      lang.current = 'es-AR';
    }
  });
});

describe('parseQuantity', () => {
  it('reads whole base units only', () => {
    expect(parseQuantity('3')).toBe(3);
    expect(parseQuantity('500')).toBe(500);
    expect(parseQuantity(' 12 ')).toBe(12);
  });

  it('refuses a fraction, because quantities are whole base units', () => {
    expect(parseQuantity('1.5')).toBeNull();
    expect(parseQuantity('0.5')).toBeNull();
    expect(parseQuantity('1,5')).toBeNull();
  });

  it('refuses zero and negatives', () => {
    expect(parseQuantity('0')).toBeNull();
    expect(parseQuantity('-3')).toBeNull();
  });

  it('refuses what is not a number', () => {
    expect(parseQuantity('')).toBeNull();
    expect(parseQuantity('abc')).toBeNull();
    expect(parseQuantity('1e3')).toBeNull();
  });
});

describe('parseInitialStock', () => {
  it('reads an empty field as zero, which is what the shelf holds', () => {
    expect(parseInitialStock('')).toBe(0);
    expect(parseInitialStock('   ')).toBe(0);
  });

  it('reads a typed zero as zero, exactly like the empty field', () => {
    // The defect this exists for. The catalog reused `parseQuantity`, which
    // refuses zero on purpose — a movement of zero units is not a movement. An
    // initial stock of zero is not a movement either, it is the absence of one,
    // and the form has to accept it instead of disabling its own button and
    // explaining the refusal with a message about the name and the price.
    expect(parseInitialStock('0')).toBe(0);
    expect(parseInitialStock(' 0 ')).toBe(0);
  });

  it('reads a positive whole number as itself', () => {
    expect(parseInitialStock('1')).toBe(1);
    expect(parseInitialStock('500')).toBe(500);
  });

  it('refuses what is not a whole, non-negative count', () => {
    expect(parseInitialStock('-1')).toBeNull();
    expect(parseInitialStock('1.5')).toBeNull();
    expect(parseInitialStock('1,5')).toBeNull();
    expect(parseInitialStock('abc')).toBeNull();
    expect(parseInitialStock('1e3')).toBeNull();
  });
});

describe('lineTotalAmount', () => {
  it('multiplies quantity by a decimal-string unit price exactly', () => {
    expect(lineTotalAmount(3, '2.50')).toBe('7.50');
  });

  it('supports bonus lines and rejects non-positive quantity or negative price', () => {
    expect(lineTotalAmount(9, '0.00')).toBe('0.00');
    expect(lineTotalAmount(0, '2.50')).toBeNull();
    expect(lineTotalAmount(-1, '2.50')).toBeNull();
    expect(lineTotalAmount(1, '-1.00')).toBeNull();
  });

  it('works on the wire format regardless of the display language', () => {
    // The arithmetic never sees a comma: it takes the API's amounts and returns
    // one, so switching language cannot change a total.
    for (const language of ['es-AR', 'en'] as const) {
      lang.current = language;
      expect(lineTotalAmount(3, '2.50')).toBe('7.50');
    }
  });
});

describe('sumAmounts', () => {
  it('adds amounts exactly, as whole cents', () => {
    expect(sumAmounts(['2.50', '7.50'])).toBe('10.00');
    expect(sumAmounts(['0.00', '0.00'])).toBe('0.00');
    expect(sumAmounts([])).toBe('0.00');
    // 0.10 + 0.20 is 0.30000000000000004 in float; as cents it is 0.30.
    expect(sumAmounts(['0.10', '0.20'])).toBe('0.30');
  });

  it('keeps the ledger footing readable for a negative sum', () => {
    expect(sumAmounts(['-1.00', '0.50'])).toBe('-0.50');
  });

  it('refuses an amount that is not one, instead of summing garbage', () => {
    expect(sumAmounts(['2.50', 'nope'])).toBeNull();
    expect(sumAmounts(['2.5'])).toBeNull();
    // An es-AR rendering is a display string, not an amount.
    expect(sumAmounts(['$ 1.234,56'])).toBeNull();
  });
});

describe('formatQuantity with its unit', () => {
  it('renders the backend unit verbatim, with no conversion', () => {
    expect(formatQuantity(1500, 'g')).toBe('1.500 g');
    expect(formatQuantity(500, 'ml')).toBe('500 ml');
    expect(formatQuantity(3, 'unit')).toBe('3');
    expect(formatQuantity(1, 'unit')).toBe('1');
  });

  it('groups a four-digit count the way English writes numbers', () => {
    lang.current = 'en';
    expect(formatQuantity(1500, 'g')).toBe('1,500 g');
  });
});

describe('dates follow the active language', () => {
  it('formats the day chip and the time in the active language', () => {
    const day = new Date(2026, 9, 16, 22, 5);
    expect(formatDay(day)).toBe('16 oct 2026');
    expect(formatTime(day)).toBe('22:05');
    lang.current = 'en';
    expect(formatDay(day)).toBe('Oct 16, 2026');
    expect(formatTime(day)).toBe('22:05');
  });
});
