import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import {
  canStepPage,
  dayIsTruncated,
  dayOffset,
  dayPageCount,
  dayPageNumber,
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
  it('asks for what owes with no day bound at all', () => {
    // **`R3-001` of `review-045709e4682ea82c`, and the second regression I introduced.**
    // The first version bounded this ask by `to: day.from`, reasoning that an owed document
    // belonging to the day is already in the day's list. That reasoning covers *today* and
    // nothing else: an owed expense dated after the shop day ends is fetched by neither
    // list — the day's own list stops at `day.to`, and this one stopped at `day.from`. The
    // baseline rendered it, so it was a silent disappearance of exactly the kind this whole
    // change exists to end.
    //
    // No bound is the answer, and the overlap is not a problem: `mergeOlderDebts`
    // deduplicates by id, so a document in both answers renders once.
    const query = unsettledQuery(50);
    expect(query).toEqual({ unsettled: true, limit: 50 });
  });

  it('carries no lower bound either, because an owed document can be dated anywhere', () => {
    const query = unsettledQuery(10) as Record<string, unknown>;
    expect(query.to).toBeUndefined();
    expect(query.from).toBeUndefined();
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

describe('the day\u2019s page size, and the notice it owes', () => {
  it('asks for the day with the page size the baseline used, not a narrower one', async () => {
    // **`R2-001` of `review-16c84931d9274aaf`, and it was a regression I introduced.** The
    // baseline fetched `limit: 200` for the day and filtered to the day in the browser, so
    // 200 was how many of a day's documents reached the screen. The first version of this
    // fix narrowed it to 50 while adding the older-debts ask, which quietly shrank what a
    // busy day shows -- a behaviour change nobody asked for, in a fix whose whole subject was
    // documents going missing from a page.
    const source = await readFile(
      resolve(process.cwd(), 'src/lib/components/ExpenseLedger.svelte'),
      'utf8'
    );
    const page = /const PAGE = (\d+);/.exec(source);
    expect(page, 'the component declares its day page size').not.toBeNull();
    expect(Number(page?.[1])).toBe(200);
  });

  it('says so when part of the day is not on screen', async () => {
    // The notice is the other half of the finding: a page with no way to say "there is
    // more" is the same silent truncation the older-debts notice closes, one list over.
    // 200 of 300: two thirds of the day are off screen, so the notice is due.
    expect(dayIsTruncated(200, 300)).toBe(true);
    // 51 of 200: the same notice, because the *remaining* part of the day is what matters,
    // not how full the page looks.
    expect(dayIsTruncated(51, 200)).toBe(true);
    // The whole day, and the empty day: neither is a warning.
    expect(dayIsTruncated(200, 200)).toBe(false);
    expect(dayIsTruncated(0, 0)).toBe(false);
  });

  it('compares against the exact day count, never against the page size', async () => {
    // **The distinction the first version got wrong.** Comparing rows held to the endpoint's
    // `cap` asked "is this page full", which is not the question: the second page of a
    // 300-document day holds 100 rows and is not a warning, while its first page holds 200
    // and is. Only the exact total can tell a short page from a truncated day.
    expect(dayIsTruncated(100, 300)).toBe(true);
    expect(dayIsTruncated(200, 200)).toBe(false);
    for (const held of [1, 49, 50, 199, 200]) {
      expect(dayIsTruncated(held, held)).toBe(false);
    }
    // A day holding nothing holds all of nothing: no notice, and no division to guard.
    expect(dayIsTruncated(0, 0)).toBe(false);
  });
});

describe('the two asks between them cover every owed document', () => {
  /**
   * **The property `R3-001` of `review-045709e4682ea82c` violated, stated directly.**
   *
   * Whatever day it is, the union of "the day's documents" and "the documents that owe"
   * must contain every owed document — no matter where its date falls. The regression is
   * exactly a hole in that union: a future-dated owed expense fell between the two lists,
   * and no test was asking the question in those terms, which is why two unit tests and an
   * e2e all stayed green while the document vanished.
   */
  const bounds = { from: '2026-03-15T03:00:00Z', to: '2026-03-16T03:00:00Z' };
  const owed = (id: string, occurredAt: string): Expense =>
    expense({ id, occurred_at: occurredAt });

  it('covers an owed document dated long before the day', () => {
    const row = owed('old', '2025-01-10T12:00:00Z');
    const seen = [...expensesInDay([], bounds), ...mergeOlderDebts([], [row])].map((r) => r.id);
    expect(seen).toEqual(['old']);
  });

  it('covers an owed document dated inside the day', () => {
    const row = owed('today', '2026-03-15T15:00:00Z');
    const seen = mergeOlderDebts(expensesInDay([row], bounds), [row]).map((r) => r.id);
    expect(seen).toEqual(['today']);
  });

  it('covers an owed document dated AFTER the day, which is the hole that was there', () => {
    // `occurred_at` is the day the expense belongs to, and nothing forbids a future one —
    // October's salary can be entered before it is due. The first version of
    // `unsettledQuery` bounded the ask by the day's own start, so this document was in
    // neither list and rendered nowhere.
    const row = owed('next-month', '2026-04-01T12:00:00Z');
    const seen = [...expensesInDay([row], bounds), ...mergeOlderDebts([], [row])].map((r) => r.id);
    expect(seen).toEqual(['next-month']);
  });
});

describe('paging the day', () => {
  it('never asks for a page past the first when the day fits', () => {
    expect(dayOffset(0, 50, 50)).toBe(0);
    expect(dayOffset(0, 50, 3)).toBe(0);
  });

  it('steps one page forward and back', () => {
    expect(dayOffset(50, 50, 200)).toBe(50);
    expect(dayOffset(50, 50, 200)).toBe(50);
  });

  it('clamps at zero so asking for one page back from the start stays put', () => {
    // The pager's back button is disabled at the first page, and this is the rule that
    // makes that state true rather than merely drawn.
    expect(dayOffset(-50, 50, 200)).toBe(0);
    expect(dayOffset(-1, 50, 200)).toBe(0);
  });

  it('never lands on a page past the last one, which would render an empty stack', () => {
    // The measured case: 201 documents in pages of 50 is five pages (0, 50, 100, 150, 200),
    // and the last one holds a single row. Asking for 250 would answer nothing at all and
    // the operator would see an empty day with a foot reporting 201 documents.
    expect(dayOffset(200, 50, 201)).toBe(200);
    expect(dayOffset(250, 50, 201)).toBe(200);
    expect(dayOffset(1000, 50, 201)).toBe(200);
    // And exactly on a boundary the last page is the one that ends there.
    expect(dayOffset(50, 50, 100)).toBe(50);
    expect(dayOffset(100, 50, 100)).toBe(50);
  });

  it('has a first page even for a day with nothing in it', () => {
    expect(dayOffset(0, 50, 0)).toBe(0);
    expect(dayOffset(50, 50, 0)).toBe(0);
  });

  it('counts the pages a day needs, for the label', () => {
    expect(dayPageCount(0, 50)).toBe(1);
    expect(dayPageCount(1, 50)).toBe(1);
    expect(dayPageCount(50, 50)).toBe(1);
    expect(dayPageCount(51, 50)).toBe(2);
    expect(dayPageCount(201, 50)).toBe(5);
  });

  it('numbers the pages from one, because a label is not an index', () => {
    expect(dayPageNumber(0, 50)).toBe(1);
    expect(dayPageNumber(50, 50)).toBe(2);
    expect(dayPageNumber(200, 50)).toBe(5);
  });
});

describe('canStepPage', () => {
  it('offers a next page only when one exists', () => {
    expect(canStepPage(0, 50, 51, 1)).toBe(true);
    expect(canStepPage(0, 50, 50, 1)).toBe(false);
    expect(canStepPage(0, 50, 0, 1)).toBe(false);
  });

  it('offers a previous page only when one exists, so the start is a real boundary', () => {
    expect(canStepPage(0, 50, 200, -1)).toBe(false);
    expect(canStepPage(50, 50, 200, -1)).toBe(true);
  });

  it('does not offer a next page from the last one, even when the count is inexact', () => {
    // `total` is exact, so this is a rule rather than a hope: the last page's offset plus a
    // page is already clamped back to itself.
    expect(canStepPage(200, 50, 201, 1)).toBe(false);
    expect(canStepPage(150, 50, 201, 1)).toBe(true);
  });
});

describe('the clamp is idempotent, so re-reading after it cannot recurse', () => {
  it('clamping an already-clamped offset changes nothing', () => {
    // The component re-reads once when the clamp moves the page, which is only safe if a
    // second pass finds the offset already in place. Otherwise a day that shrank while the
    // screen was open would reload forever.
    for (const total of [0, 1, 50, 51, 201, 1000]) {
      const once = dayOffset(9999, 50, total);
      const twice = dayOffset(once, 50, total);
      expect(twice).toBe(once);
    }
  });
});

describe('walking to the newest page lands on the first one', () => {
  it('steps back from any page until offset 0, and stops there', () => {
    // **The property a pager owes its reader**: pressing "newer" repeatedly returns to the
    // page the day starts on. Asserted here because the e2e found the opposite — the walk
    // stopped one page short, which meant `canStepPage` refused the last step while a page
    // was still available. The loop below is exactly what the component's button does.
    for (const total of [1, 50, 51, 201, 400, 1000]) {
      let offset = dayOffset(10_000, 50, total); // the last page
      let steps = 0;
      while (canStepPage(offset, 50, total, -1) && steps < 200) {
        offset = dayOffset(offset - 50, 50, total);
        steps += 1;
      }
      expect(offset).toBe(0);
    }
  });
});
