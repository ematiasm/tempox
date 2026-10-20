/**
 * The running balance the day's ledger carries in its left margin.
 *
 * The signature move of the statement world is that a column of balances reads
 * as one continuous number the eye can follow down the page — the "saldo
 * corriente" of a printed account. That only holds if each balance is the sum
 * of everything above it, so this arithmetic cannot live inside a component:
 * inside `Ledger.svelte` no unit test could reach it, and a balance that is
 * merely "the amount" repeated is the failure mode that looks correct in a
 * screenshot.
 *
 * Two rules this module exists to keep:
 *
 * 1. A **cancelled** sale contributes nothing. The ledger deliberately keeps it
 *    visible, struck through, so the correction can be read — but it never
 *    moved money, so the balance must not move with it.
 * 2. A row that does **not** count still shows the balance it inherits. Its
 *    cell reads the same number as the row above rather than an empty space,
 *    because a blank in the column breaks the continuous read the column is
 *    for.
 *
 * The ledger lists newest first. A printed statement reads down from the
 * oldest, so the balance column is built oldest-first and then handed back in
 * the ledger's own order — computing it top-down in display order would sum the
 * day backwards.
 *
 * Every function is pure and takes plain amounts as strings, matching the wire
 * format the rest of the arithmetic uses.
 */
import type { Amount } from './locale.js';

/** The one fact about a row this module needs. */
export type BalanceRow = {
  readonly total: Amount;
  /** Whether the document counts toward the day. A cancelled sale does not. */
  readonly counts: boolean;
};

/**
 * The balance after each row, in the same order the rows were given.
 *
 * `rows` is oldest-first. The result is parallel to it, so the caller reverses
 * both together or neither.
 */
export function runningBalances(rows: readonly BalanceRow[]): Amount[] {
  const balances: Amount[] = [];
  let sum = 0;
  for (const row of rows) {
    if (row.counts) sum += toNumber(row.total);
    balances.push(fromCents(sum));
  }
  return balances;
}

/**
 * The day's total: what the ledger says the till took.
 *
 * Summed from the same rows the balances come from, so the footer can never
 * disagree with the column above it — a foot that adds up differently from its
 * own column is the one error a statement never makes.
 */
export function dayTotal(rows: readonly BalanceRow[]): Amount {
  let sum = 0;
  for (const row of rows) {
    if (row.counts) sum += toNumber(row.total);
  }
  return fromCents(sum);
}

/**
 * Amounts arrive as decimal strings (`"12400.00"`). This reads them as integer
 * cents so the sum never touches a float: `0.1 + 0.2` is real money lost, which
 * is the reason the whole system stores decimals in the first place.
 *
 * A malformed amount counts as zero rather than throwing. The server owns
 * validation; a screen that throws because one row is odd is worse than one
 * whose balance is visibly short, and the shortfall is the signal.
 */
function toNumber(amount: Amount): number {
  const value = Number(amount);
  if (!Number.isFinite(value)) return 0;
  return Math.round(value * 100);
}

/** Cents back to the wire format's two decimals, never a float's represenation. */
function fromCents(cents: number): Amount {
  return (cents / 100).toFixed(2);
}
