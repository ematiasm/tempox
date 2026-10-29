/**
 * Paging a ledger, for every ledger that is a page.
 *
 * **Three screens load a capped page and used to handle that fact three different ways.**
 * The expenses stack ended at a wall until it gained a pager; the sales ledger told the reader
 * its table was partial and gave them nowhere to go; the purchases ledger neither told nor
 * paged — it asked for 200 rows and let the rest of the day go. None of those is a bug in one
 * screen. They are one arithmetic living in three places, done correctly zero times.
 *
 * So this module owns the arithmetic and nothing else, exactly as `methodLabel.ts` owns the
 * rule for "what does an empty rail cell say" across the same three ledgers. It is named for
 * the ledger rather than for the day because sales and purchases page a *period* whose width
 * the reader chooses, while expenses page a day.
 *
 * **The clamp is the load-bearing function.** `total` is an exact count the server answered —
 * the expenses summary's `documents`, the sales summary's `documents` — never a page-derived
 * guess. With an exact total, the last page is a known address, and a pager that cannot exceed
 * it cannot ask for a page that holds nothing. Without that, a reader who leaves a screen open
 * while the period shrinks (a refund, a deleted draft, another tab) ends up on an address past
 * the end and sees an empty table beneath a footer reporting documents.
 */

/** One page's worth of rows, whichever ledger is asking. */
export type PageState = {
  /** The row offset of the page on screen. */
  offset: number;
  /** How many rows one page holds — the `limit` the caller sent. */
  size: number;
  /** The exact number of documents the server says the whole span holds. */
  total: number;
};

/**
 * Where a page request starts, clamped to the pages that actually hold something.
 *
 * Clamped at both ends, and the two clamps answer different mistakes:
 *
 * * **Below zero** is a step back from the first page. The button is disabled there, so this
 *   clamp is what makes that state true rather than merely drawn.
 * * **Past the last page** is a remembered offset outliving the rows it addressed. A ledger
 *   left open while its period shrinks would otherwise render an empty table under a footer
 *   that reports documents — a screen contradicting its own figures.
 */
export function clampOffset(current: number, size: number, total: number): number {
  const lastPage = Math.max(0, (pageCount(total, size) - 1) * size);
  return Math.min(Math.max(0, current), lastPage);
}

/** How many pages a span needs. Never zero: every span has a first page, even an empty one. */
export function pageCount(total: number, size: number): number {
  if (total <= 0 || size <= 0) return 1;
  return Math.ceil(total / size);
}

/** The page a label shows, numbered from one — an offset is an index, a label is not. */
export function pageNumber(offset: number, size: number): number {
  if (size <= 0) return 1;
  return Math.floor(Math.max(0, offset) / size) + 1;
}

/**
 * Whether a pager step is possible in that direction, so a button can be disabled honestly.
 *
 * Asked as "would this step land somewhere else" rather than "is this offset past a bound",
 * because both ends are then one expression. A disabled button that *would* have moved is the
 * failure this prevents: the expenses pager shipped one that stopped a page short, and the e2e
 * is what caught it.
 */
export function canStepPage(offset: number, size: number, total: number, delta: number): boolean {
  return clampOffset(offset + delta * size, size, total) !== clampOffset(offset, size, total);
}

/** Whether the page on screen leaves part of the span out. */
export function pageIsPartial(rowsHeld: number, total: number): boolean {
  return rowsHeld < total;
}

/**
 * Whether a page response still describes the page the reader is looking at.
 *
 * **Both CRITICAL findings of `review-58ab3200d6f59c90` were this rule missing**, and the
 * validator's follow-up was that the test asserting it defined its own copy instead of calling
 * production code — indirect coverage of a rule the components implement separately, which is
 * the decoration class in a subtler costume.
 *
 * The rule is one comparison, and it exists as a named function because two components need it
 * and both got it wrong in different ways:
 *
 * * purchases passed `() => false` as its staleness predicate from `stepPage`, which reads like
 *   a delegation to the generation counter and means *nothing is ever stale*;
 * * sales assigned a post-refund reload to `rows` without comparing the page it was requested
 *   for against the page now on screen.
 *
 * `requested` is captured **before** the request is sent and compared **after** it returns.
 * Anything else — reading the current offset at both ends — compares a value with itself.
 */
export function isSamePageResponse(requested: number, current: number): boolean {
  return requested === current;
}
