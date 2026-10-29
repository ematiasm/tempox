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

/**
 * Whether the page being shown leaves part of the day out.
 *
 * **A page with no way to say "there is more" is the same silent truncation the older-debts
 * notice closes, one list over** (`R2-001` of `review-16c84931d9274aaf`).
 *
 * Its first version compared the rows held against the endpoint's `cap`, which was the honest
 * question while there was no pager: fewer rows than the cap meant the whole day by
 * construction. **It is the wrong question now**, and the difference is worth stating because
 * the two answers disagree exactly where it matters. With a pager, a full first page of a
 * 300-document day holds 200 rows and `200 >= 200` is true — but so does the *second* page,
 * whose 100 remaining rows also compare as `100 >= 200 == false`. The cap cannot tell "the
 * day is longer than a page" from "this page is short", and the second is not a warning.
 *
 * The count that answers it is the summary's `documents`, which is an **exact** count over
 * the day rather than a capped one. `rowsHeld < total` is then true precisely when some part
 * of the day is not on screen, whichever page is being shown.
 */
export function dayIsTruncated(rowsHeld: number, total: number): boolean {
  return rowsHeld < total;
}

/**
 * Where the day's next page starts, clamped to the pages that actually hold something.
 *
 * **The day's list has always been a page, and until now the screen could only say so.**
 * `dayIsTruncated` above tells the operator "this day holds more documents than fit here",
 * which is honest and leaves them nowhere to go: 200 documents was a wall. This is the
 * arithmetic the pager needs, and it lives here rather than in the component for the reason
 * every other rule in this module does — a `$effect` is not a place anything can be asserted.
 *
 * The clamp at the end is the one that matters. `total` comes from the summary's `documents`,
 * which is an **exact** count over the day rather than a capped one, so the last page is a
 * known address. Without the clamp, a pager could ask for offset 250 of a 201-document day,
 * receive an empty `rows`, and render an empty stack underneath a foot reporting 201
 * documents — a screen contradicting its own figures, which is the failure this whole slice
 * exists to remove.
 */
export function dayOffset(current: number, page: number, total: number): number {
  const lastPage = Math.max(0, (dayPageCount(total, page) - 1) * page);
  return Math.min(Math.max(0, current), lastPage);
}

/** How many pages a day needs. Never zero: every day has a first page, even an empty one. */
export function dayPageCount(total: number, page: number): number {
  if (total <= 0 || page <= 0) return 1;
  return Math.ceil(total / page);
}

/** The page a label shows, numbered from one — an offset is an index, a label is not. */
export function dayPageNumber(offset: number, page: number): number {
  if (page <= 0) return 1;
  return Math.floor(Math.max(0, offset) / page) + 1;
}

/** Whether a pager step is possible in that direction, so a button can be disabled honestly. */
export function canStepPage(offset: number, page: number, total: number, delta: number): boolean {
  return dayOffset(offset + delta * page, page, total) !== dayOffset(offset, page, total);
}
