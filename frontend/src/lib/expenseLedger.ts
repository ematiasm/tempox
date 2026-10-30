/**
 * The expenses screen's arithmetic, and nothing else.
 *
 * **This file exists because that arithmetic used to live inside a `$effect` and was
 * wrong** (issue #115, `R3-003` of `review-0e3269a3a9623ddc`). The ledger asked the server
 * for the newest 200 documents in the shop's whole history and then decided, in the
 * browser, which of them were today's and which were older documents that still owe. Past
 * 200 documents an owed salary older than that page rendered no row, and with it no advance
 * form — the same defect `R3-003` of `review-1e9c4c74ced6d94b` had already closed once,
 * moved one page size further out, with no sign on screen that anything was missing.
 *
 * The fix is not a larger page. **A question about documents is asked of the database**, so
 * the server gained `unsettled` — "the documents that still owe something" — and this
 * module only combines two answers that are already true:
 *
 * ```text
 * today's documents  = GET /api/expenses?from=<day>&to=<next day>     (the day's own list)
 * older debts        = GET /api/expenses?unsettled=true&to=<day from>  (its own page)
 * ```
 *
 * Why `to=<day from>` and not a flag: an owed document that belongs to the day being read is
 * **already** in the first list, and asking for it again would render it twice or count it
 * twice. `to` is exclusive in this API, so passing the day's own first instant excludes the
 * whole day in one parameter that already meant exactly this.
 */

import type { Expense } from '#lib/api.js';
import {
  canStepPage as canStepPageIn,
  clampOffset,
  pageCount,
  pageIsPartial,
  pageNumber
} from '#lib/ledgerPaging.js';

/** The two bounds a shop day is described by, both ISO 8601 UTC instants. */
export type DayBounds = { from: string; to: string };

/**
 * The request that asks the server for the older debts.
 *
 * `limit` is the page this screen is willing to render, not a horizon it hopes is enough:
 * the response carries `unsettled_older`, so a page that is not the whole is **sayable** —
 * which is the difference between this and the fix it replaces.
 *
 * Returned as a plain object rather than a URL so the rule can be asserted without a
 * server, and so `api.ts` keeps owning how a query is serialized (one place that knows
 * `location`, which is what keeps the module importable during SSR).
 */
export function unsettledQuery(limit: number): { unsettled: true; limit: number } {
  // **No `from` and no `to`, deliberately** (`R3-001` of `review-045709e4682ea82c`). The
  // first version bounded this ask by `to: day.from` so the day's documents would not be
  // asked for twice — and that reasoning covered *today only*. An owed expense dated after
  // the shop day ends was fetched by neither list: the day's own list stops at `day.to`, and
  // this one stopped at `day.from`. It rendered before this change and disappeared after it,
  // which is the single defect this whole slice exists to end.
  //
  // The overlap the bound was avoiding is already handled where it belongs: `mergeOlderDebts`
  // deduplicates by id, so a document that answers both questions renders once.
  return { unsettled: true, limit };
}

/** The documents of a day, given its half-open `[from, to)` bounds. */
export function expensesInDay(rows: Expense[], day: DayBounds): Expense[] {
  return rows.filter((row) => row.occurred_at >= day.from && row.occurred_at < day.to);
}

/**
 * The day's stack: the day's own documents first, then the older ones that still owe.
 *
 * **Order is the server's**, kept in both halves rather than re-sorted here: `/api/expenses`
 * orders by `occurred_at DESC, id`, so the day reads newest first and the older debts follow
 * in the same order. Re-sorting would be a second implementation of an ordering the server
 * already guarantees, and the two would eventually disagree.
 *
 * Deduplicated by id because the two reads are not one snapshot: a document created between
 * them can legitimately answer both, and rendering it twice would place two advance forms
 * over one debt.
 */
export function mergeOlderDebts(day: Expense[], older: Expense[]): Expense[] {
  const seen = new Set(day.map((row) => row.id));
  return [...day, ...older.filter((row) => !seen.has(row.id))];
}

/**
 * The figure in the page header.
 *
 * **The day's count, never the merged list's.** The header sits above a foot that reports
 * the day's spending, and counting the merged list made the two describe different screens:
 * a day holding two expenses displayed seven documents because five unrelated old debts were
 * rendered under them. `null` until the day has been read, so the header shows nothing rather
 * than a zero it has not earned — the same rule the purchases ledger follows.
 */
export function ledgerDocumentCount(dayCount: number | null, _mergedCount: number): number | null {
  if (dayCount === null) return null;
  return dayCount;
}

// ── The day's pager ────────────────────────────────────────────────────────
//
// **The arithmetic is not here any more, and that is the fix rather than a tidy-up.** It was
// written once for this screen, and the sales and purchases ledgers each had their own answer
// to the same question — one announced the truncation and offered nowhere to go, the other
// neither announced nor paged. `ledgerPaging.ts` owns it now, for the three of them and for the
// next one, exactly as `methodLabel.ts` owns the rule for an empty rail cell. These wrappers
// keep the day's own vocabulary (`dayPageOffset`, `dayPageCount`) so this module still reads as
// being about a day, while there is only one implementation underneath.

/** Where the day's page request starts, clamped to the pages that hold something. */
export function dayOffset(current: number, page: number, total: number): number {
  return clampOffset(current, page, total);
}

/** How many pages the day needs. Never zero: an empty day has a first page. */
export function dayPageCount(total: number, page: number): number {
  return pageCount(total, page);
}

/** The page the label shows, numbered from one. */
export function dayPageNumber(offset: number, page: number): number {
  return pageNumber(offset, page);
}

/** Whether a pager step is possible, so the button can be disabled honestly. */
export function canStepPage(offset: number, page: number, total: number, delta: number): boolean {
  return canStepPageIn(offset, page, total, delta);
}

/**
 * Whether the page on screen leaves part of the day out.
 *
 * **The rule moved to `ledgerPaging.ts`** (`pageIsPartial`), because the sales and purchases
 * ledgers need the same answer and each had its own: sales announced the truncation and gave
 * the reader nowhere to go, purchases neither announced nor paged. This wrapper keeps the
 * day's name for it.
 *
 * **Its first version asked against the endpoint's `cap`** — `rowsHeld >= cap`, "is this page
 * full" — which was the honest question while there was no pager and stopped being the question
 * once one existed: the second page of a 300-document day holds 100 rows and also compares
 * false, so a reader paging through a long day would lose the warning half way. The count that
 * answers it is the summary's exact `documents`.
 */
export function dayIsTruncated(rowsHeld: number, total: number): boolean {
  return pageIsPartial(rowsHeld, total);
}
