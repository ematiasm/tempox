import { describe, expect, it } from 'vitest';
import { esAR, en } from '#lib/locale.js';

/**
 * The locale's two directions, per language.
 *
 * Every expectation here was read off a real `Intl` in Node 26 rather than
 * written from memory — an assertion about a localized string that was guessed
 * is a test that documents a guess.
 */
describe('money formatting per locale (S4)', () => {
  it('writes es-AR amounts with a space, dot grouping and a decimal comma', () => {
    expect(esAR.money('25.00')).toBe('$ 25,00');
    expect(esAR.money('0.01')).toBe('$ 0,01');
    expect(esAR.money('0.00')).toBe('$ 0,00');
    expect(esAR.money('1234.56')).toBe('$ 1.234,56');
    expect(esAR.money('184300.00')).toBe('$ 184.300,00');
    expect(esAR.money('-7200.00')).toBe('-$ 7.200,00');
  });

  it('keeps the en form exactly as it was before the locale existed', () => {
    // This is the preserved-behavior half: the strings the e2e suite asserted
    // before this slice must still be the strings it asserts now.
    expect(en.money('25.00')).toBe('$25.00');
    expect(en.money('0.01')).toBe('$0.01');
    expect(en.money('0.00')).toBe('$0.00');
    expect(en.money('184300.00')).toBe('$184,300.00');
    expect(en.money('-7200.00')).toBe('-$7,200.00');
  });

  it('never loses exactness on an amount a float could not hold', () => {
    // 0.1 + 0.2 in float is 0.30000000000000004, and a large amount divided by
    // 100 drifts. This path only ever slices a string, so the digits that came
    // in are the digits that come out.
    expect(esAR.money('99999999999999.99')).toBe('$ 99.999.999.999.999,99');
    expect(en.money('99999999999999.99')).toBe('$99,999,999,999,999.99');
  });
});

describe('amount input per locale (S5)', () => {
  it('reads the es-AR forms a cashier actually types', () => {
    expect(esAR.parseAmount('1234,56')).toBe('1234.56');
    expect(esAR.parseAmount('1.234,56')).toBe('1234.56');
    expect(esAR.parseAmount('1.234')).toBe('1234.00');
    expect(esAR.parseAmount('25')).toBe('25.00');
    expect(esAR.parseAmount('25,5')).toBe('25.50');
    expect(esAR.parseAmount('0,01')).toBe('0.01');
    expect(esAR.parseAmount('$ 1.234,56')).toBe('1234.56');
    expect(esAR.parseAmount('  1.234,56  ')).toBe('1234.56');
  });

  it('refuses what it cannot read unambiguously', () => {
    // The one that matters: `1,234.56` is the en form. Under es-AR it reads as
    // 1.23456 (or as nothing), and silently accepting it would record a price
    // a thousand times off. A refusal is the only safe answer.
    expect(esAR.parseAmount('1,234.56')).toBeNull();
    expect(esAR.parseAmount('1.23.4,56')).toBeNull();
    expect(esAR.parseAmount('1.234,5,6')).toBeNull();
    expect(esAR.parseAmount('0,005')).toBeNull();
    expect(esAR.parseAmount('1.999,999')).toBeNull();
    expect(esAR.parseAmount('-5')).toBeNull();
    expect(esAR.parseAmount('')).toBeNull();
    expect(esAR.parseAmount('   ')).toBeNull();
    expect(esAR.parseAmount('abc')).toBeNull();
    expect(esAR.parseAmount('1e3')).toBeNull();
  });

  it('accepts a value copied off the screen, in both languages (deliberate widening)', () => {
    // The base refused `$ 25.00` (symbol followed by a space). That refusal made
    // "copy the figure and paste it back" fail, so the strip is now uniform. This
    // is the one place `en` input is deliberately wider than the base commit, and
    // it is recorded here rather than left as a silent difference (S17).
    expect(en.parseAmount('$ 25.00')).toBe('25.00');
    expect(en.parseAmount(' $ 1,234.56 ')).toBe('1234.56');
    expect(esAR.parseAmount('$ 1.234,56')).toBe('1234.56');
    // It stays a *parse*, not a coercion: a space inside the digits is not a
    // thousands separator in either language.
    expect(en.parseAmount('1 234.56')).toBeNull();
    expect(esAR.parseAmount('1 234,56')).toBeNull();
  });

  it('keeps the en reading unchanged otherwise', () => {
    expect(en.parseAmount('25')).toBe('25.00');
    expect(en.parseAmount('25.00')).toBe('25.00');
    expect(en.parseAmount('25.5')).toBe('25.50');
    expect(en.parseAmount('1,234.56')).toBe('1234.56');
    expect(en.parseAmount('$25.00')).toBe('25.00');
    expect(en.parseAmount('0.005')).toBeNull();
    expect(en.parseAmount('2.5.0')).toBeNull();
    expect(en.parseAmount('-5')).toBeNull();
    expect(en.parseAmount('')).toBeNull();
    expect(en.parseAmount('1e3')).toBeNull();
  });

  it('round-trips each locale through its own formatter', () => {
    // What the screen shows can be typed back in and means the same thing. This
    // is the property that makes the two directions a pair instead of two
    // functions that happen to agree today.
    for (const amount of ['0.00', '0.01', '0.99', '1.00', '25.00', '1234.56', '184300.00']) {
      expect(esAR.parseAmount(esAR.money(amount))).toBe(amount);
      expect(en.parseAmount(en.money(amount))).toBe(amount);
    }
  });

  it('does not round-trip one locale through the other, and does not pretend to', () => {
    // The wire form is en-shaped in both directions. Reading an es-AR rendering
    // as en is a refusal, not a silent misread — which is exactly why the
    // parser is chosen by the active language rather than guessed from the text.
    expect(en.parseAmount(esAR.money('1234.56'))).toBeNull();
  });
});

describe('quantities and units per locale (S11)', () => {
  it('groups counts the way each language writes numbers', () => {
    expect(esAR.quantity(1500)).toBe('1.500');
    expect(en.quantity(1500)).toBe('1,500');
    expect(esAR.quantity(3)).toBe('3');
    expect(en.quantity(3)).toBe('3');
  });

  it('renders a measured quantity with its symbol verbatim, never converted', () => {
    expect(esAR.quantityWithUnit(1500, 'g')).toBe('1.500 g');
    expect(en.quantityWithUnit(1500, 'g')).toBe('1,500 g');
    expect(esAR.quantityWithUnit(500, 'ml')).toBe('500 ml');
    expect(en.quantityWithUnit(500, 'ml')).toBe('500 ml');
  });

  it('renders a unit-product quantity as the count alone, in both languages', () => {
    // Unchanged on purpose (S17): the column header says what is counted, and
    // the ledger has always read `3`, not `3 units`.
    expect(esAR.quantityWithUnit(3, 'unit')).toBe('3');
    expect(en.quantityWithUnit(3, 'unit')).toBe('3');
  });

  it('localizes the unit *label*, not the unit *value*', () => {
    expect(esAR.unit('unit')).toBe('unidad');
    expect(en.unit('unit')).toBe('unit');
    // A symbol is a symbol in both languages; translating `g` to `g` is the
    // correct answer, and converting it to kg is not.
    expect(esAR.unit('g')).toBe('g');
    expect(en.unit('g')).toBe('g');
    expect(esAR.unit('ml')).toBe('ml');
  });

  it('offers an example amount in the very shape its own parser accepts', () => {
    // The regression this pins: the price field showed `0.00` while the es-AR
    // parser refused it, so a cashier who copied the hint could not create a
    // product at all -- the button stayed disabled with no explanation.
    expect(esAR.amountHint()).toBe('0,00');
    expect(en.amountHint()).toBe('0.00');
    expect(esAR.parseAmount(esAR.amountHint())).toBe('0.00');
    expect(en.parseAmount(en.amountHint())).toBe('0.00');
  });

  it('writes a wire amount for the field to read back, and reads it back', () => {
    // The defect this closes: the draft prefilled the price field with the wire
    // form, so under es-AR the line did not parse and the sale could not be
    // recorded at all -- the button stayed disabled with no explanation.
    expect(esAR.amountInput('25.00')).toBe('25,00');
    expect(esAR.parseAmount(esAR.amountInput('25.00'))).toBe('25.00');
    expect(esAR.amountInput('1234.56')).toBe('1234,56');
    expect(esAR.parseAmount(esAR.amountInput('1234.56'))).toBe('1234.56');
    // English is the wire form, so it passes through untouched.
    expect(en.amountInput('25.00')).toBe('25.00');
    expect(en.parseAmount(en.amountInput('25.00'))).toBe('25.00');
    // Round-trip for every amount the app can show.
    for (const amount of ['0.00', '0.01', '1.00', '25.00', '184300.00']) {
      expect(esAR.parseAmount(esAR.amountInput(amount))).toBe(amount);
      expect(en.parseAmount(en.amountInput(amount))).toBe(amount);
    }
  });

  it('labels a price field with the locale currency and the unit', () => {
    expect(esAR.priceUnit('unit')).toBe('$ / unidad');
    expect(en.priceUnit('unit')).toBe('$/unit');
    // No conversion of the unit, only its label: a gram price stays a gram price.
    expect(esAR.priceUnit('g')).toBe('$ / g');
    expect(en.priceUnit('g')).toBe('$/g');
  });
});

describe('a document time is rendered in the shop zone (S19)', () => {
  it('reads a late-evening sale on the shop clock, not the machine clock', () => {
    // 2026-10-17T01:30:00Z is 22:30 on 16 Oct in Buenos Aires. Rendering it with
    // the browser's zone would put 01:30 inside a page the server had correctly
    // scoped to 16 Oct — the defect this spec closes.
    const saleAt = new Date('2026-10-17T01:30:00Z');
    expect(esAR.time(saleAt, 'America/Argentina/Buenos_Aires')).toBe('22:30');
    expect(en.time(saleAt, 'America/Argentina/Buenos_Aires')).toBe('22:30');
    // The same instant in a zone five hours behind reads on that zone's clock.
    expect(esAR.time(saleAt, 'America/Chicago')).toBe('20:30');
  });

  it('handles a zone whose offset changes with the season, with no offset maths', () => {
    // A fixed offset is wrong on the two days a year a zone changes it. `Intl`
    // carries the tz database, so the same wall-clock hour resolves differently
    // in summer and in winter -- and the test pins that, so an "optimization"
    // back to a cached number fails.
    const summer = new Date('2026-07-10T16:00:00Z'); // 12:00 EDT
    const winter = new Date('2026-01-10T17:00:00Z'); // 12:00 EST
    expect(esAR.time(summer, 'America/New_York')).toBe('12:00');
    expect(esAR.time(winter, 'America/New_York')).toBe('12:00');
  });

  it('falls back to the browser zone only when the shop zone is unknown', () => {
    // First paint, before `/api/day` answers, has no zone to use. The fallback is
    // the previous behaviour and is never invented data.
    const at = new Date(2026, 9, 16, 22, 5);
    expect(esAR.time(at)).toBe('22:05');
    expect(en.time(at)).toBe('22:05');
  });
});

describe('dates per locale (S6)', () => {
  it('writes the day chip the way each language writes a date', () => {
    const day = new Date(2026, 9, 16, 15, 0);
    expect(esAR.day(day)).toBe('16 oct 2026');
    expect(en.day(day)).toBe('Oct 16, 2026');
  });

  it('writes a 24-hour time in both languages', () => {
    const evening = new Date(2026, 9, 16, 22, 5);
    expect(esAR.time(evening)).toBe('22:05');
    expect(en.time(evening)).toBe('22:05');
    const morning = new Date(2026, 9, 16, 9, 7);
    expect(esAR.time(morning)).toBe('09:07');
    expect(en.time(morning)).toBe('09:07');
  });
});
