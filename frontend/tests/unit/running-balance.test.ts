import { describe, it, expect } from 'vitest';
import { runningBalances, dayTotal, type BalanceRow } from '#lib/runningBalance.js';

/**
 * The balance column is the redesign's signature move, and it is arithmetic:
 * every rule here is one a screenshot cannot catch. A column that repeats each
 * row's own amount looks right in a picture and is wrong on paper.
 *
 * The ledger hands these rows newest-first on screen, but `runningBalances` is
 * specified oldest-first — the order a printed statement reads down — so every
 * fixture here is written the way a statement is read and the caller's reversal
 * is a separate concern.
 */

const row = (total: string, counts = true): BalanceRow => ({ total, counts });

describe('runningBalances', () => {
  it('accumulates down the column, oldest first', () => {
    // The day in reading order: 09:42, 09:58, 10:04, 10:31.
    const rows = [row('1890.00'), row('6180.00'), row('8590.00'), row('12400.00')];

    expect(runningBalances(rows)).toEqual(['1890.00', '8070.00', '16660.00', '29060.00']);
  });

  it('does not move the balance for a cancelled sale, but still reports it', () => {
    // V-000003 was annulled. It stays visible and struck through, and the
    // balance must read the same number as the row above: money that never
    // moved cannot appear in a running total.
    const rows = [row('6180.00'), row('3940.00', false), row('8590.00')];

    expect(runningBalances(rows)).toEqual(['6180.00', '6180.00', '14770.00']);
  });

  it('carries an inherited balance through a cancelled row at the start', () => {
    // A cancelled document before anything counted has nothing to inherit:
    // the column starts at zero rather than at the cancelled amount.
    const rows = [row('3940.00', false), row('1000.00')];

    expect(runningBalances(rows)).toEqual(['0.00', '1000.00']);
  });

  it('is empty for an empty day', () => {
    expect(runningBalances([])).toEqual([]);
  });

  it('keeps two decimals so a cent is never rounded away', () => {
    const rows = [row('0.10'), row('0.20')];

    // 0.1 + 0.2 in floating point is 0.30000000000000004. Money is not.
    expect(runningBalances(rows)).toEqual(['0.10', '0.30']);
  });

  it('treats a malformed amount as zero instead of throwing', () => {
    const rows = [row('nope'), row('1000.00')];

    expect(runningBalances(rows)).toEqual(['0.00', '1000.00']);
  });
});

describe('dayTotal', () => {
  it('sums only the rows that count', () => {
    const rows = [row('6180.00'), row('3940.00', false), row('8590.00'), row('12400.00')];

    expect(dayTotal(rows)).toBe('27170.00');
  });

  it('agrees with the last balance of the column', () => {
    // The footer and the column are the same number or the statement is wrong.
    const rows = [row('1890.00'), row('6180.00'), row('8590.00'), row('12400.00')];
    const balances = runningBalances(rows);

    expect(dayTotal(rows)).toBe(balances.at(-1));
  });

  it('is zero for an empty day, not NaN', () => {
    expect(dayTotal([])).toBe('0.00');
  });

  it('is zero when every row was cancelled', () => {
    expect(dayTotal([row('3940.00', false), row('1000.00', false)])).toBe('0.00');
  });
});
