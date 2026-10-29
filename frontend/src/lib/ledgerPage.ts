/**
 * The one owner of a ledger's page.
 *
 * **This module exists because patching the page state one path at a time produced the same
 * defect five times.** Three review rounds over the expenses/purchases/sales pagers found, in
 * order: a ledger that passed `() => false` as its staleness predicate, so nothing was ever
 * stale; a post-write reload that assigned rows without asking whether the reader had paged; a
 * clamp that invalidated *its own* re-read by comparing against the offset it had just changed;
 * a period that was not part of the comparison; and a rollback that read the offset *after* the
 * step had moved it, so it "restored" the value it was meant to undo. `odd/tasks/v4-expenses.md`
 * L25 records all five.
 *
 * The common cause was never any one of them. It was that **the offset lived in the component,
 * so every asynchronous path had to remember to capture, compare and roll back** — five paths,
 * five chances to forget, and the compiler could not help because each capture was a local.
 *
 * So the offset lives here, in one closure, and a component never assigns it. Every read goes
 * through `read`, which captures the context **where the read is issued** rather than where its
 * result is consumed, and discards its own result when that context no longer holds. The rollback
 * in `step` works because the previous offset was captured before the step and never left this
 * module — there is no argument for a caller to forget.
 *
 * The fetcher is a parameter rather than an import: the three screens read different endpoints
 * with different shapes. What they share is not the request, it is the question *"does this
 * answer still describe what the reader is looking at"* — and that question has one answer.
 */

import { clampOffset } from '#lib/ledgerPaging.js';

/** One page of rows, plus the exact total the whole span holds. */
export type PageResult<Row> = {
  rows: Row[];
  /** Exact document count for the entire span — never derived from this page. */
  total: number;
};

/**
 * Reads the page at `offset`, and returns it with whatever else the screen needs.
 *
 * The extra value is how a component carries its own per-read data (suppliers, cash, totals)
 * through the owner without the owner knowing what it is.
 */
export type PageFetcher<Row, Extra> = (
  offset: number
) => Promise<{ page: PageResult<Row>; extra: Extra }>;

/** Applies an accepted read. Never called for a discarded one. */
export type PageApply<Row, Extra> = (rows: Row[], extra: Extra, total: number) => void;

export type LedgerPage<Row, Extra> = {
  /** The page on screen. Read-only to the component: there is no setter. */
  readonly offset: number;
  /** The span's exact total, as of the last read that was applied. */
  readonly total: number;
  /**
   * Runs one read for `span` at the current page.
   *
   * Answers `true` when the result was applied and `false` when it was discarded as superseded,
   * so a caller can tell "reloaded" from "someone else owns the screen now" without re-deriving
   * it from a local comparison.
   */
  read(
    span: unknown,
    fetcher: PageFetcher<Row, Extra>,
    apply: PageApply<Row, Extra>
  ): Promise<boolean>;
  /**
   * Moves a page and reads it, rolling back to the page whose rows are still on screen when the
   * read fails.
   */
  step(
    delta: number,
    span: unknown,
    fetcher: PageFetcher<Row, Extra>,
    apply: PageApply<Row, Extra>,
    onError: (e: unknown) => void
  ): Promise<void>;
};

export function createLedgerPage<Row, Extra>(size: number): LedgerPage<Row, Extra> {
  let offset = 0;
  let total = 0;
  let generation = 0;

  /**
   * The only read.
   *
   * Every discard reason lives in one place, and they are checked in the order that makes them
   * cheap: a newer read first, then the span, then the page. The five patches this replaces each
   * remembered a different subset.
   */
  async function read(
    span: unknown,
    fetcher: PageFetcher<Row, Extra>,
    apply: PageApply<Row, Extra>
  ): Promise<boolean> {
    return readWithin(++generation, span, fetcher, apply);
  }

  async function readWithin(
    generationFor: number,
    span: unknown,
    fetcher: PageFetcher<Row, Extra>,
    apply: PageApply<Row, Extra>
  ): Promise<boolean> {
    // **Captured at issue time.** `mine` is what this read is allowed to answer for.
    const mine = { offset, span, generation: generationFor };
    const { page, extra } = await fetcher(mine.offset);

    // A newer read owns the screen now.
    if (mine.generation !== generation) return false;
    // A different span: the answer belongs to a period the reader has left (`R3-002` of
    // `review-811cd5f5c6e59feb` — the offset alone cannot see this).
    if (!Object.is(mine.span, span)) return false;
    // A different page: the reader stepped while this was in flight (`R3-002` of
    // `review-58ab3200d6f59c90`).
    if (mine.offset !== offset) return false;

    // **The clamp, and its re-read is issued by this module** (`R3-001` of
    // `review-811cd5f5c6e59feb`). The component's version changed the offset and then re-entered
    // its loader with a predicate that compared against the offset captured before the request,
    // so the clamp made the predicate true and the re-read was thrown away. Here the re-read is a
    // fresh generation issued after the offset settled, so there is nothing to compare wrongly.
    const clamped = clampOffset(mine.offset, size, page.total);
    if (clamped !== mine.offset) {
      offset = clamped;
      total = page.total;
      // The re-read belongs to the same logical read, so it keeps `mine`'s generation: a reader
      // who steps during it must be caught by the **offset** guard, not by a newer generation.
      // This is what makes that guard load-bearing rather than redundant with the counter.
      return readWithin(mine.generation, span, fetcher, apply);
    }

    total = page.total;
    apply(page.rows, extra, total);
    return true;
  }

  async function step(
    delta: number,
    span: unknown,
    fetcher: PageFetcher<Row, Extra>,
    apply: PageApply<Row, Extra>,
    onError: (e: unknown) => void
  ): Promise<void> {
    const next = clampOffset(offset + delta * size, size, total);
    if (next === offset) return;

    // **`R3-003`, closed by construction.** Captured before the step and never handed to a
    // caller, so the rollback cannot restore the offset the step just set — which is exactly what
    // the component version did: it read `pageOffset` *after* the step had already moved it.
    const previous = offset;
    const previousTotal = total;
    offset = next;
    try {
      await read(span, fetcher, apply);
    } catch (e) {
      offset = previous;
      total = previousTotal;
      onError(e);
    }
  }

  return {
    get offset() {
      return offset;
    },
    get total() {
      return total;
    },
    read,
    step
  };
}
