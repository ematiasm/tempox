import { describe, it, expect } from 'vitest';
import { runningBalances, dayTotal, netOf, type BalanceRow } from '#lib/runningBalance.js';

/**
 * The balance column is the redesign's signature move, and it is arithmetic:
 * every rule here is one a screenshot cannot catch. A column that repeats each
 * row's own amount looks right in a picture and is wrong on paper.
 *
 * The ledger hands these rows newest-first on screen, but `runningBalances` is
 * specified oldest-first — the order a printed statement reads down — so every
 * fixture here is written the way a statement is read and the caller's reversal
 * is a separate concern.
 *
 * `refunded` is part of every fixture, not an optional extra. The first version
 * of this module summed gross totals and the native review found the defect by
 * asking what a *partial* refund contributes: a 100.00 sale with a 30.00 credit
 * note moved 70.00 through the till, and the balance said 100.00.
 */

const row = (total: string, refunded = '0.00', counts = true): BalanceRow => ({
  total,
  refunded,
  counts
});

describe('netOf', () => {
  it('is the gross total when nothing came back', () => {
    expect(netOf(row('100.00'))).toBe(10000);
  });

  it('subtracts a partial refund', () => {
    // The defect the review found: 100.00 sold, 30.00 returned, 70.00 kept.
    expect(netOf(row('100.00', '30.00'))).toBe(7000);
  });

  it('is zero for a document given back in full', () => {
    expect(netOf(row('100.00', '100.00'))).toBe(0);
  });

  it('is zero for a document that does not count', () => {
    expect(netOf(row('100.00', '0.00', false))).toBe(0);
  });

  it('never goes negative when the record and the screen disagree', () => {
    // The server prevents over-refunding. If this ever reads negative the two
    // disagree, and a negative contribution would *reduce* a running total of
    // money the shop kept — worse than a visibly short balance.
    expect(netOf(row('100.00', '130.00'))).toBe(0);
  });
});

describe('runningBalances', () => {
  it('accumulates down the column, oldest first', () => {
    // The day in reading order: 09:42, 09:58, 10:04, 10:31.
    const rows = [row('1890.00'), row('6180.00'), row('8590.00'), row('12400.00')];

    expect(runningBalances(rows)).toEqual(['1890.00', '8070.00', '16660.00', '29060.00']);
  });

  it('moves the balance by the net, not the gross, on a partial refund', () => {
    // The whole point of the fix, in one assertion: the column must follow the
    // money the shop kept.
    const rows = [row('100.00'), row('100.00', '30.00'), row('50.00')];

    expect(runningBalances(rows)).toEqual(['100.00', '170.00', '220.00']);
  });

  it('does not move the balance for a cancelled sale, but still reports it', () => {
    // V-000003 was annulled. It stays visible and struck through, and the
    // balance must read the same number as the row above: money that never
    // moved cannot appear in a running total.
    const rows = [row('6180.00'), row('3940.00', '0.00', false), row('8590.00')];

    expect(runningBalances(rows)).toEqual(['6180.00', '6180.00', '14770.00']);
  });

  it('carries an inherited balance through a cancelled row at the start', () => {
    const rows = [row('3940.00', '0.00', false), row('1000.00')];

    expect(runningBalances(rows)).toEqual(['0.00', '1000.00']);
  });

  it('holds the balance through a document given back in full', () => {
    // A full refund already contributes zero; the column must not dip and
    // recover, because the shop never lost money it had not taken.
    const rows = [row('100.00'), row('100.00', '100.00'), row('50.00')];

    expect(runningBalances(rows)).toEqual(['100.00', '100.00', '150.00']);
  });

  it('is empty for an empty period', () => {
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
  it('sums the nets of the rows that count', () => {
    const rows = [row('6180.00'), row('3940.00', '0.00', false), row('8590.00'), row('12400.00')];

    expect(dayTotal(rows)).toBe('27170.00');
  });

  it('agrees with the last balance of the column', () => {
    // The footer and the column are the same number or the statement is wrong.
    const rows = [row('1890.00'), row('6180.00'), row('8590.00'), row('12400.00')];
    const balances = runningBalances(rows);

    expect(dayTotal(rows)).toBe(balances.at(-1));
  });

  it('agrees with the column even when a row was partly refunded', () => {
    const rows = [row('100.00'), row('100.00', '30.00'), row('50.00', '20.00')];
    const balances = runningBalances(rows);

    expect(dayTotal(rows)).toBe('200.00');
    expect(dayTotal(rows)).toBe(balances.at(-1));
  });

  it('is zero for an empty period, not NaN', () => {
    expect(dayTotal([])).toBe('0.00');
  });

  it('is zero when every row was cancelled or given back in full', () => {
    expect(dayTotal([row('3940.00', '0.00', false), row('1000.00', '1000.00')])).toBe('0.00');
  });
});
