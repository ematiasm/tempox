import { describe, expect, it } from 'vitest';
import {
  formatCents,
  formatQuantity,
  lineTotal,
  parseAmountToCents,
  parseQuantity
} from '#lib/money.js';

describe('formatCents', () => {
  it('renders integer cents as en-US money', () => {
    expect(formatCents(2500)).toBe('$25.00');
    expect(formatCents(1)).toBe('$0.01');
    expect(formatCents(0)).toBe('$0.00');
    expect(formatCents(18430000)).toBe('$184,300.00');
  });

  it('keeps a negative amount readable, for the cash foot', () => {
    expect(formatCents(-720000)).toBe('-$7,200.00');
  });
});

describe('parseAmountToCents', () => {
  it('reads what a cashier types as whole cents', () => {
    expect(parseAmountToCents('25')).toBe(2500);
    expect(parseAmountToCents('25.00')).toBe(2500);
    expect(parseAmountToCents('25.5')).toBe(2550);
    expect(parseAmountToCents('0.01')).toBe(1);
    expect(parseAmountToCents('0')).toBe(0);
    expect(parseAmountToCents('1,234.56')).toBe(123456);
    expect(parseAmountToCents(' 25.00 ')).toBe(2500);
  });

  it('accepts zero, because a bonus line is a real line', () => {
    // plan-v1a, S12: `unit_price: 0` is recorded and contributes nothing to
    // cash. A validator that rejects zero makes the regalo undeliverable.
    expect(parseAmountToCents('0.00')).toBe(0);
  });

  it('refuses a sub-cent amount instead of rounding it away silently', () => {
    // Prices are integer cents per base unit. $0.005 is not expressible, and
    // quietly turning it into $0.01 or $0.00 would record a price the cashier
    // never typed.
    expect(parseAmountToCents('0.005')).toBeNull();
    expect(parseAmountToCents('1.999')).toBeNull();
  });

  it('refuses anything that is not an amount', () => {
    expect(parseAmountToCents('')).toBeNull();
    expect(parseAmountToCents('   ')).toBeNull();
    expect(parseAmountToCents('abc')).toBeNull();
    expect(parseAmountToCents('2.5.0')).toBeNull();
    expect(parseAmountToCents('-5')).toBeNull();
    expect(parseAmountToCents('1e3')).toBeNull();
  });

  it('round-trips every formatted value', () => {
    for (const cents of [0, 1, 99, 100, 2500, 123456, 18430000]) {
      expect(parseAmountToCents(formatCents(cents))).toBe(cents);
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
    // 1.5 does not exist: a gram product's quantity is 1 or 2 grams, and a
    // unit product's is 1 or 2 pieces.
    expect(parseQuantity('1.5')).toBeNull();
    expect(parseQuantity('0.5')).toBeNull();
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

describe('lineTotal', () => {
  it('multiplies quantity by the per-base-unit price', () => {
    expect(lineTotal(3, 2500)).toBe(7500);
    expect(lineTotal(500, 1)).toBe(500);
  });

  it('is zero for a bonus line, and never negative', () => {
    expect(lineTotal(9, 0)).toBe(0);
    expect(lineTotal(-1, 100)).toBe(0);
    expect(lineTotal(1, -100)).toBe(0);
  });
});

describe('formatQuantity with its unit', () => {
  it('renders the backend unit verbatim, with no kg or L conversion', () => {
    // S4: `1500 g` displays as `1500 g`. A client that converts to kilograms
    // invents a unit the API never agreed to send.
    expect(formatQuantity(1500, 'g')).toBe('1,500 g');
    expect(formatQuantity(500, 'ml')).toBe('500 ml');
    expect(formatQuantity(3, 'unit')).toBe('3');
    expect(formatQuantity(1, 'unit')).toBe('1');
  });
});
