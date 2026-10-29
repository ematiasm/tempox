import { describe, expect, it } from 'vitest';
import {
  expensesInDay,
  ledgerDocumentCount,
  mergeOlderDebts,
  unsettledQuery
} from '#lib/expenseLedger.js';
import type { Expense } from '#lib/api.js';

/**
 * How the expenses screen decides what to render (issue #115).
 *
 * **The bug was a filter in the wrong place.** `ExpenseLedger.svelte` asked the server for
 * the newest 200 documents in the shop's whole history and then filtered that page in the
 * browser: today's rows came from it, and so did "every older document that still owes".
 * Past 200 documents an owed salary older than the page rendered no row and no advance
 * form — the exact defect `R3-003` of `review-1e9c4c74ced6d94b` had already closed once,
 * moved one page size further out and with nothing on screen to say so.
 *
 * The rule that replaces it is not "ask for more". It is that **a question about documents
 * is asked of the database**, and the screen's only remaining arithmetic is combining two
 * answers that are already true: the day's own list, and the older debts the server found.
 *
 * These tests pin the three pieces that arithmetic needs from a former `$effect`'s inline
 * body — the query it sends, the merge it performs, and the count it reports — because a
 * component-level assertion would need a browser and would still be asserting the same
 * three things.
 */

function expense(over: Partial<Expense> & { id: string }): Expense {
  return {
    category_id: 'cat-1',
    category_name: 'Sueldos',
    payee: 'someone',
    amount: '300.00',
    occurred_at: '2026-03-15T12:00:00Z',
    payment_method_id: null,
    payment_method_name: null,
    paid: '0.00',
    outstanding: '300.00',
    created_at: '2026-03-15T12:00:01Z',
    ...over
  } as Expense;
}

describe('unsettledQuery', () => {
  it('asks for what owes, below the day, in one page of its own', () => {
    // The `to` bound is the day's own first instant: an owed document that belongs to the
    // day being read is already in the day's list, and asking for it twice would render it
    // twice or count it twice.
    const query = unsettledQuery({ from: '2026-03-15T03:00:00Z', to: '2026-03-16T03:00:00Z' }, 50);
    expect(query).toEqual({ unsettled: true, to: '2026-03-15T03:00:00Z', limit: 50 });
  });

  it('never sends the day bound it was given for the end of the day', () => {
    // A regression guard with a reason: sending `to` instead of `from` would make the
    // question "owed documents *in* this day", which is the set the merge already has.
    const query = unsettledQuery({ from: 'from-instant', to: 'to-instant' }, 10);
    expect(query.to).toBe('from-instant');
    expect(query.to).not.toBe('to-instant');
  });
});

describe('mergeOlderDebts', () => {
  it('keeps the day in the shop order and puts older debts after it', () => {
    const day = [expense({ id: 'b', occurred_at: '2026-03-15T14:00:00Z' }), expense({ id: 'a' })];
    const older = [expense({ id: 'old-1', occurred_at: '2025-01-10T12:00:00Z' })];

    const merged = mergeOlderDebts(day, older);
    expect(merged.map((row) => row.id)).toEqual(['b', 'a', 'old-1']);
  });

  it('does not render a document twice when the server answers it in both lists', () => {
    // Possible in principle — the two reads are not one snapshot — and a duplicate would
    // render a second advance form for one debt.
    const shared = expense({ id: 'both' });
    const merged = mergeOlderDebts([shared], [shared, expense({ id: 'old-1' })]);
    expect(merged.map((row) => row.id)).toEqual(['both', 'old-1']);
  });

  it('is the day alone when nothing older owes', () => {
    const day = [expense({ id: 'a' })];
    expect(mergeOlderDebts(day, []).map((row) => row.id)).toEqual(['a']);
  });
});

describe('expensesInDay', () => {
  it('keeps only the documents inside the half-open bounds', () => {
    const rows = [
      expense({ id: 'before', occurred_at: '2026-03-15T02:59:59Z' }),
      expense({ id: 'first', occurred_at: '2026-03-15T03:00:00Z' }),
      expense({ id: 'last', occurred_at: '2026-03-16T02:59:59Z' }),
      expense({ id: 'after', occurred_at: '2026-03-16T03:00:00Z' })
    ];
    const inDay = expensesInDay(rows, {
      from: '2026-03-15T03:00:00Z',
      to: '2026-03-16T03:00:00Z'
    });
    expect(inDay.map((row) => row.id)).toEqual(['first', 'last']);
  });
});

describe('ledgerDocumentCount', () => {
  it("reports the day's own count, not the merged list", () => {
    // The header's figure sits above a foot that reports the day's spending. Counting the
    // merged list made it report the day plus unrelated older debts — a day with two
    // expenses displayed "7 documents" over a foot that covered two.
    expect(ledgerDocumentCount(2, 5)).toBe(2);
  });

  it('is null before the day has been read, so the header shows nothing rather than a zero', () => {
    expect(ledgerDocumentCount(null, 0)).toBeNull();
  });
});
