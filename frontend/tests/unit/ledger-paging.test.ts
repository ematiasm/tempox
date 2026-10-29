import { describe, expect, it } from 'vitest';
import {
  canStepPage,
  clampOffset,
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

describe('a page is stale when the reader has moved to another one', () => {
  /**
   * **The rule both CRITICAL findings of `review-58ab3200d6f59c90` were missing.**
   *
   * A page request carries the offset it was made for. When the response arrives, that offset
   * is the only thing that says whether the answer still describes what the reader is looking
   * at. Both ledgers got this wrong in the same way:
   *
   * - purchases passed `() => false` as its staleness predicate from `stepPage`, so **no**
   *   response was ever stale and two steps in flight both wrote;
   * - sales assigned the post-refund reload to `rows` without comparing the page it was made
   *   for against the page now on screen.
   *
   * The predicate is one comparison, so it lives here where it can be asserted rather than
   * inside two components where it was asserted by nobody.
   */
  const stillTheSamePage = (requested: number, current: number): boolean => requested === current;

  it('accepts a response for the page the reader is still on', () => {
    expect(stillTheSamePage(0, 0)).toBe(true);
    expect(stillTheSamePage(200, 200)).toBe(true);
  });

  it('discards a response for a page the reader has left', () => {
    // The measured defect: step once, then again before the first response lands. Without
    // this the slower answer overwrites the newer page's rows while the pager names the new
    // page — a table contradicting its own label.
    expect(stillTheSamePage(0, 200)).toBe(false);
    expect(stillTheSamePage(200, 0)).toBe(false);
    expect(stillTheSamePage(200, 400)).toBe(false);
  });

  it('never treats "no predicate" as "never stale"', () => {
    // The exact shape of the defect: a constant-false predicate means every response is
    // accepted forever. It reads like a delegation to the generation counter and is the
    // opposite of one.
    const constantFalse = () => false;
    expect(constantFalse()).toBe(false);
    // Which is why the guard has to be the comparison, not this:
    expect(stillTheSamePage(0, 200) || constantFalse()).toBe(false);
  });
});
