import { describe, expect, it } from 'vitest';
import {
  formatMoney,
  formatQuantity,
  lineTotalAmount,
  parseAmount,
  parseQuantity,
  sumAmounts
} from '#lib/money.js';

describe('formatMoney', () => {
  it('renders exact decimal amounts as en-US currency', () => {
    expect(formatMoney('25.00')).toBe('$25.00');
    expect(formatMoney('0.01')).toBe('$0.01');
    expect(formatMoney('0.00')).toBe('$0.00');
    expect(formatMoney('184300.00')).toBe('$184,300.00');
    expect(formatMoney('-7200.00')).toBe('-$7,200.00');
  });
});

describe('parseAmount', () => {
  it('normalizes valid cashier input to a two-decimal amount', () => {
    expect(parseAmount('25')).toBe('25.00');
    expect(parseAmount('25.00')).toBe('25.00');
    expect(parseAmount('25.5')).toBe('25.50');
    expect(parseAmount('0.01')).toBe('0.01');
    expect(parseAmount('0')).toBe('0.00');
    expect(parseAmount('1,234.56')).toBe('1234.56');
    expect(parseAmount(' 25.00 ')).toBe('25.00');
    expect(parseAmount('0.00')).toBe('0.00');
  });

  it('rejects sub-cent amounts under the API contract, not a display rule', () => {
    expect(parseAmount('0.005')).toBeNull();
    expect(parseAmount('1.999')).toBeNull();
  });

  it('rejects text that is not an amount', () => {
    for (const text of ['', '   ', 'abc', '2.5.0', '-5', '1e3']) {
      expect(parseAmount(text)).toBeNull();
    }
  });

  it('round-trips amount strings through display formatting', () => {
    for (const amount of ['0.00', '0.01', '0.99', '1.00', '25.00', '1234.56', '184300.00']) {
      expect(parseAmount(formatMoney(amount))).toBe(amount);
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
  });
});

describe('formatQuantity with its unit', () => {
  it('renders the backend unit verbatim, with no conversion', () => {
    expect(formatQuantity(1500, 'g')).toBe('1,500 g');
    expect(formatQuantity(500, 'ml')).toBe('500 ml');
    expect(formatQuantity(3, 'unit')).toBe('3');
    expect(formatQuantity(1, 'unit')).toBe('1');
  });
});
