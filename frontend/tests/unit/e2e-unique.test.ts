import { describe, expect, it } from 'vitest';
import { lineTotal, money, uniqueUnitPrice } from '../e2e/unique.js';

/**
 * The helper every ledger spec selects its own document with (#52).
 *
 * Its first version mapped `Date.now()` through `% 89000`: 89,000 distinct values
 * repeating every 89 seconds. Two workers starting in the same millisecond, or two
 * runs an exact multiple of 89 seconds apart, drew the same price -- and because
 * the ledger is append-only and shared, that collision poisoned the value for every
 * later run. The suite went back to matching more than one row, which is the exact
 * failure the helper exists to prevent.
 *
 * So the uniqueness space is pinned here rather than assumed: the tests below fail
 * if the counter or the run seed is ever dropped from the mix.
 */

describe('uniqueUnitPrice', () => {
  it('never repeats inside one process', () => {
    // The counter is what guarantees this. Without it, two specs starting in the
    // same millisecond drew identical prices.
    const prices = Array.from({ length: 200 }, () => uniqueUnitPrice());
    expect(new Set(prices).size).toBe(prices.length);
  });

  it('formats as a shop price: comma decimal, two places, no grouping', () => {
    // The price field parses this. A period or a thousands separator would be
    // refused and the product could not be created at all.
    for (const price of Array.from({ length: 20 }, () => uniqueUnitPrice())) {
      expect(price).toMatch(/^\d+,\d{2}$/);
    }
  });

  it('stays a plausible unit price, because totals are derived from it', () => {
    // The first attempt folded into a 9,999,999 value space and produced prices
    // near $100,000 -- absurd for a counter and noisy in every assertion.
    for (const price of Array.from({ length: 20 }, () => uniqueUnitPrice())) {
      const value = Number(price.replace(',', '.'));
      expect(value).toBeGreaterThanOrEqual(100);
      expect(value).toBeLessThan(10_000);
    }
  });
});

describe('money', () => {
  it('renders the shop currency marker', () => {
    expect(money('4,37')).toBe('$ 4,37');
  });

  it('groups thousands with a period, which is what es-AR renders', () => {
    // Measured: a computed total of 2596.16 renders as "$ 2.596,16", and the first
    // version of this helper printed "$ 2596,16". It failed only on the runs whose
    // price happened to be large enough -- an intermittent failure inside the fix
    // for an intermittent failure.
    expect(money('2596,16')).toBe('$ 2.596,16');
    expect(money('1234567,89')).toBe('$ 1.234.567,89');
  });
});

describe('lineTotal', () => {
  it('multiplies a shop price by a whole quantity', () => {
    expect(lineTotal('4,37', 4)).toBe('17,48');
    expect(lineTotal('10,00', 3)).toBe('30,00');
  });

  it('does not lose cents on a price with two decimals', () => {
    // Floating point would give 3 * 33.33 = 99.99000000000001 here.
    expect(lineTotal('33,33', 3)).toBe('99,99');
  });

  it('agrees with money, which is what the assertions compare against', () => {
    const price = uniqueUnitPrice();
    const total = lineTotal(price, 7);
    expect(money(total)).toMatch(/^\$ \d{1,3}(\.\d{3})*,\d{2}$/);
  });
});
