import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import {
  canStepPage,
  clampOffset,
  isSamePageResponse,
  isStaleAfterClamp,
  offsetAfterFailedLoad,
  pageCount,
  pageIsPartial,
  pageNumber
} from '#lib/ledgerPaging.js';

/**
 * Paging arithmetic, shared by the three ledgers that are pages.
 *
 * This module exists because the same arithmetic was written three times and done correctly
 * zero times: expenses had no pager, sales announced the truncation and offered nowhere to go,
 * purchases neither announced nor paged. These tests are the rule, once, so the three screens
 * cannot disagree about it again.
 */

describe('clampOffset', () => {
  it('never asks for a page past the last one that holds something', () => {
    // 201 documents in pages of 200 is two pages; the second holds one row. Asking for 400
    // would answer nothing and render an empty table under a footer reporting 201 documents.
    expect(clampOffset(0, 200, 201)).toBe(0);
    expect(clampOffset(200, 200, 201)).toBe(200);
    expect(clampOffset(400, 200, 201)).toBe(200);
    expect(clampOffset(10_000, 200, 201)).toBe(200);
  });

  it('never asks for a negative offset, so the first page is a real boundary', () => {
    expect(clampOffset(-200, 200, 1000)).toBe(0);
    expect(clampOffset(-1, 50, 1000)).toBe(0);
  });

  it('lands exactly on the last page when the count divides evenly', () => {
    expect(clampOffset(100, 50, 100)).toBe(50);
    expect(clampOffset(200, 50, 100)).toBe(50);
  });

  it('has a first page for a span holding nothing', () => {
    expect(clampOffset(0, 50, 0)).toBe(0);
    expect(clampOffset(500, 50, 0)).toBe(0);
  });

  it('is idempotent, so a caller that re-reads after clamping cannot loop', () => {
    // The expenses ledger re-reads once when the clamp moves the page. That is only safe
    // because a second pass always finds the offset already in place.
    for (const total of [0, 1, 50, 51, 200, 201, 1000]) {
      for (const size of [50, 200]) {
        const once = clampOffset(9999, size, total);
        expect(clampOffset(once, size, total)).toBe(once);
      }
    }
  });

  it('survives a nonsensical page size rather than dividing by zero', () => {
    expect(clampOffset(500, 0, 100)).toBe(0);
    expect(clampOffset(500, -10, 100)).toBe(0);
  });
});

describe('pageCount', () => {
  it('counts the pages a span needs', () => {
    expect(pageCount(0, 200)).toBe(1);
    expect(pageCount(1, 200)).toBe(1);
    expect(pageCount(200, 200)).toBe(1);
    expect(pageCount(201, 200)).toBe(2);
    expect(pageCount(1000, 200)).toBe(5);
  });

  it('is never zero, because every span has a first page', () => {
    expect(pageCount(0, 0)).toBe(1);
    expect(pageCount(-5, 200)).toBe(1);
  });
});

describe('pageNumber', () => {
  it('numbers from one, because a label is not an index', () => {
    expect(pageNumber(0, 200)).toBe(1);
    expect(pageNumber(200, 200)).toBe(2);
    expect(pageNumber(800, 200)).toBe(5);
  });
});

describe('canStepPage', () => {
  it('offers a next page only when one exists', () => {
    expect(canStepPage(0, 200, 201, 1)).toBe(true);
    expect(canStepPage(0, 200, 200, 1)).toBe(false);
    expect(canStepPage(0, 200, 0, 1)).toBe(false);
  });

  it('offers a previous page only when one exists', () => {
    expect(canStepPage(0, 200, 1000, -1)).toBe(false);
    expect(canStepPage(200, 200, 1000, -1)).toBe(true);
  });

  it('does not offer a next page from the last one', () => {
    expect(canStepPage(200, 200, 201, 1)).toBe(false);
    expect(canStepPage(0, 200, 201, 1)).toBe(true);
  });

  it('lets a walk reach every page and the first one, from anywhere', () => {
    // **The property the expenses pager shipped without**, and the e2e caught it stopping a
    // page short: a disabled button that would have moved is a reader stranded. Asserted over
    // awkward totals because the off-by-one only shows up at the boundaries.
    for (const total of [1, 50, 51, 200, 201, 400, 401, 1000]) {
      const size = 200;
      let offset = clampOffset(10_000, size, total);
      let steps = 0;
      while (canStepPage(offset, size, total, -1) && steps < 200) {
        offset = clampOffset(offset - size, size, total);
        steps += 1;
      }
      expect(offset).toBe(0);
      // And the same walk forward reaches the last page and no further.
      steps = 0;
      while (canStepPage(offset, size, total, 1) && steps < 200) {
        offset = clampOffset(offset + size, size, total);
        steps += 1;
      }
      expect(offset).toBe(clampOffset(10_000, size, total));
    }
  });
});

describe('pageIsPartial', () => {
  it('is true whenever part of the span is off screen, whichever page is shown', () => {
    // The sales ledger said this and offered no way to see the rest; the purchases ledger
    // never said it. The rule is the same for both, and it does not depend on the page being
    // full: the second page of a 300-document span holds 100 rows and is still partial.
    expect(pageIsPartial(200, 300)).toBe(true);
    expect(pageIsPartial(100, 300)).toBe(true);
    expect(pageIsPartial(51, 200)).toBe(true);
  });

  it('is false only when the page is the whole span', () => {
    expect(pageIsPartial(200, 200)).toBe(false);
    expect(pageIsPartial(0, 0)).toBe(false);
    expect(pageIsPartial(51, 51)).toBe(false);
  });
});

describe('isSamePageResponse: the rule both CRITICAL findings were missing', () => {
  /**
   * **`R3-001` and `R3-002` of `review-58ab3200d6f59c90`.**
   *
   * A page request carries the offset it was made for. When the response arrives, that offset
   * is the only thing that says whether the answer still describes what the reader is looking
   * at. Both ledgers got it wrong, in different ways:
   *
   * - purchases passed `() => false` as `progress`'s staleness predicate from `stepPage`, so
   *   **no** response was ever stale and two steps in flight both wrote;
   * - sales assigned the post-refund reload to `rows` without comparing the page it was made
   *   for against the page now on screen.
   *
   * The first version of this test defined its own local copy of the comparison and asserted
   * that — which is what the validator's follow-up called out: indirect coverage of a rule the
   * components implement themselves. It calls the production function now.
   */
  it('accepts a response for the page the reader is still on', () => {
    expect(isSamePageResponse(0, 0)).toBe(true);
    expect(isSamePageResponse(200, 200)).toBe(true);
  });

  it('discards a response for a page the reader has left', () => {
    // The measured defect: step once, then again before the first response lands. Without
    // this the slower answer overwrites the newer page's rows while the pager names the new
    // page — a table contradicting its own label.
    expect(isSamePageResponse(0, 200)).toBe(false);
    expect(isSamePageResponse(200, 0)).toBe(false);
    expect(isSamePageResponse(200, 400)).toBe(false);
  });

  it('is the owner that applies it for purchases, and still the component for sales', async () => {
    // **The call sites moved, and this test moved with them.** Purchases no longer compares pages
    // itself: `LedgerPage` does it, from the context captured where the read is issued. Sales has
    // not been migrated yet, so it still calls the predicate directly. Asserting the current state
    // of each is what makes the remaining migration visible instead of invisible.
    const [owner, purchases, sales] = await Promise.all([
      readFile(resolve(process.cwd(), 'src/lib/ledgerPage.svelte.ts'), 'utf8'),
      readFile(resolve(process.cwd(), 'src/lib/components/PurchaseLedger.svelte'), 'utf8'),
      readFile(resolve(process.cwd(), 'src/lib/components/Ledger.svelte'), 'utf8')
    ]);
    // The owner is where the comparison lives now.
    expect(owner).toMatch(/mine\.offset !== this\.offset/);
    // Purchases owns no page comparison of its own any more — the whole point of the migration.
    expect(purchases).not.toMatch(/isSamePageResponse\(/);
    expect(purchases).toMatch(/new LedgerPage/);
    // Sales is the remaining migration; when it happens, this assertion is the reminder.
    expect(sales).toMatch(/isSamePageResponse\(/);
  });
});

describe('isStaleAfterClamp: the clamp must not invalidate its own re-read', () => {
  // **The reasoning this helper encoded, kept for the record.** `LedgerPage` now issues the
  // clamp's re-read itself and keeps the generation, so the predicate this function was written
  // for is no longer called by any component. It stays because the arithmetic is still true and
  // because deleting it would erase why the deadlock happened.
  const day = '2027-03-15T03:00:00.000Z';

  it('accepts a re-read the clamp itself caused', () => {
    expect(isStaleAfterClamp(7, 7, day, day)).toBe(false);
  });

  it('still discards a re-read that a newer load has superseded', () => {
    expect(isStaleAfterClamp(7, 8, day, day)).toBe(true);
  });

  it('still discards a re-read after the day changed', () => {
    expect(isStaleAfterClamp(7, 7, day, '2027-03-16T03:00:00.000Z')).toBe(true);
  });
});

describe('offsetAfterFailedLoad: the rollback the owner now performs itself', () => {
  it('is the identity the owner applies by restoring its captured previous offset', () => {
    // `R3-003`: the component read the offset *after* the step moved it and called that the
    // rollback. `LedgerPage.step` captures `previous` before the move, so this pure function is
    // now only the statement of what a rollback means — pinned by `ledger-page.test.ts`, which
    // deletes the restore and watches two tests fail.
    expect(offsetAfterFailedLoad(0)).toBe(0);
    expect(offsetAfterFailedLoad(200)).toBe(200);
    expect(offsetAfterFailedLoad(400)).toBe(400);
  });
});
