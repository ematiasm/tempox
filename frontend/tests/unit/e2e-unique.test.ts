import { describe, expect, it } from 'vitest';
import { lineTotal, money } from '../e2e/unique.js';

/**
 * The money helpers the ledger specs assert with (#52).
 *
 * The document-*selection* part of this module is `recordedDocumentId`, which needs
 * a browser and is covered by the e2e specs themselves. What is unit-tested here
 * is the arithmetic that formats the amounts those specs compare against, because
 * both of its defects were silent: a missing thousands separator failed only on the
 * runs whose amount was large enough, and an off-by-a-cent total would fail only on
 * prices whose multiplication is inexact in floating point.
 */

describe('money', () => {
  it('renders the shop currency marker', () => {
    expect(money('4,37')).toBe('$ 4,37');
    expect(money('0,00')).toBe('$ 0,00');
  });

  it('groups thousands with a period, which is what es-AR renders', () => {
    // Measured: a computed total of 2596.16 renders as "$ 2.596,16", and the first
    // version of this helper printed "$ 2596,16". It failed only on the runs whose
    // amount happened to be large enough -- an intermittent failure inside the fix
    // for an intermittent failure.
    expect(money('2596,16')).toBe('$ 2.596,16');
    expect(money('1234567,89')).toBe('$ 1.234.567,89');
    expect(money('999,99')).toBe('$ 999,99');
    expect(money('1000,00')).toBe('$ 1.000,00');
  });
});

describe('lineTotal', () => {
  it('multiplies a shop price by a whole quantity', () => {
    expect(lineTotal('4,37', 4)).toBe('17,48');
    expect(lineTotal('10,00', 3)).toBe('30,00');
    expect(lineTotal('25,00', 1)).toBe('25,00');
  });

  it('does not lose a cent to floating point', () => {
    // 3 * 33.33 is 99.99000000000001 in binary floating point; the cents-first
    // multiplication is what keeps it exact.
    expect(lineTotal('33,33', 3)).toBe('99,99');
    expect(lineTotal('0,07', 3)).toBe('0,21');
    expect(lineTotal('19,99', 7)).toBe('139,93');
  });

  it('round-trips through money, which is what the assertions compare', () => {
    const total = lineTotal('4,37', 4);
    expect(money(total)).toBe('$ 17,48');
  });
});
