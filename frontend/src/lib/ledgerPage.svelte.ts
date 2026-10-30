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
 * **Why a class with runes and not a closure.** The first version of this module was a plain
 * closure, and it could not be wired: `let offset` inside a function is not Svelte state, so
 * reading `page.offset` from a template created **no reactive dependency** — the pager would
 * have rendered once and then never updated. Svelte 5 only tracks runes, and runes only work in
 * `.svelte.ts`. So the owner is a class whose fields are `$state`, which keeps the encapsulation
 * (a component reads `offset` and never assigns it) *and* makes the reads observable.
 *
 * The fetcher is a parameter rather than an import: the three screens read different endpoints
 * with different shapes. What they share is not the request, it is the question *"does this
 * answer still describe what the reader is looking at"* — and that question has one answer.
 */

import { untrack } from 'svelte';
import { clampOffset } from './ledgerPaging.js';

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

export class LedgerPage<Row, Extra> {
  /** The page on screen. **Read-only to components**: there is no setter. */
  offset = $state(0);
  /** The span's exact total, as of the last read that was applied. */
  total = $state(0);

  /**
   * The page whose rows are actually on screen, which is what a failed read must return to.
   *
   * **`S21`.** Not the offset captured when the read was issued: by then the page has already moved,
   * so restoring it restores the value it was meant to undo — the mistake `R3-003` of
   * `review-811cd5f5c6e59feb` recorded and the one this field exists to make unexpressible. Written
   * only where a read is applied.
   */
  #applied = { offset: 0, total: 0 };

  /**
   * Not `$state`: a claim is compared, never rendered. Making it reactive would re-run every
   * effect that reads the page whenever a read starts, which is the opposite of what it is for.
   */
  #claim = 0;
  readonly #size: number;

  constructor(size: number) {
    this.#size = size;
  }

  /**
   * Claims the screen for a read the caller is about to issue, and returns the token. **The owner
   * holds the failure as well as the answer**: a rejected read never reaches `#readWithin`, so the
   * caller's own error and loading guard had to be a second, hand-written predicate — which is how
   * one of them was lost (`R3-003`).
   */
  claim(): number {
    return ++this.#claim;
  }

  /** Whether the request issued under `token` is still the one that owns the screen. */
  owns(token: number): boolean {
    return token === this.#claim;
  }

  /**
   * Runs one read for `span` at the current page, answering whether it was applied. Pass the
   * `token` from `claim` to gate the caller's own error and loading flag too; omitted, it claims.
   */
  async read(
    span: unknown,
    fetcher: PageFetcher<Row, Extra>,
    apply: PageApply<Row, Extra>,
    token: number = this.claim()
  ): Promise<boolean> {
    return this.#readWithin(token, span, fetcher, apply);
  }

  async #readWithin(
    token: number,
    span: unknown,
    fetcher: PageFetcher<Row, Extra>,
    apply: PageApply<Row, Extra>
  ): Promise<boolean> {
    // **Captured at issue time.** `mine` is what this read is allowed to answer for.
    //
    // **And read with `untrack`, here rather than in the component** (`S22`). The capture happens
    // synchronously inside the caller's effect, so reading `this.offset` plainly subscribed that
    // effect to it — and moving the page then re-ran the effect, which claimed *after* the step and
    // superseded it. Untracking at the source is what makes one reader possible at all.
    const mine = untrack(() => ({ offset: this.offset, span, generation: token }));
    const { page, extra } = await fetcher(mine.offset);

    // **One guard, not three** (`R3-001`/`#120`). The span check compared a captured value with the
    // argument its own call was made with, so it could never discard anything; it is deleted. The
    // offset check stays: the clamp's re-read keeps the claim, so a step during it has no new token.
    if (!this.owns(mine.generation)) return false;
    if (mine.offset !== this.offset) return false;

    // **The clamp, and its re-read is issued by this class** (`R3-001` of
    // `review-811cd5f5c6e59feb`). The component's version changed the offset and then re-entered
    // its loader with a predicate that compared against the offset captured before the request,
    // so the clamp made the predicate true and the re-read was thrown away — a logical deadlock.
    // Here the re-read keeps the same generation deliberately: a reader who steps during it is
    // then caught by the **offset** guard, which is what makes that guard more than decoration.
    const clamped = clampOffset(mine.offset, this.#size, page.total);
    if (clamped !== mine.offset) {
      this.total = page.total;
      this.offset = clamped;
      return this.#readWithin(token, span, fetcher, apply);
    }

    this.total = page.total;
    this.#applied = { offset: this.offset, total: this.total };
    apply(page.rows, extra, this.total);
    return true;
  }

  /**
   * Marks the page as needing a re-read, without reading it.
   *
   * **For a screen whose only reader is its own effect.** Sales is one: the effect fetches the
   * period, so a write that lands while the screen is open must not start a second read — the two
   * would race for the generation and the discarded one might be the one carrying the new rows.
   * `refresh` bumps `revision`, the effect depends on `revision`, and the screen has exactly one
   * reader.
   */
  refresh(): void {
    this.revision += 1;
  }

  /**
   * The signal a screen's effect reads to know it should re-fetch.
   *
   * Written **only** by `refresh` and `stepTo`. It is deliberately the single trigger: a screen
   * whose effect depended on both the offset and a token would run for two reasons and could not
   * tell which, which is how a stale read slipped past the previous attempt.
   */
  revision = $state(0);

  /**
   * Moves the page **without reading it** — the only way a screen moves a page.
   *
   * **`S20`: one ledger, one reader.** The effect reacts to `revision`, so moving the offset and
   * the revision together is the whole interaction and the one reader fetches. The imperative
   * `step` this class used to carry was the defect, not a second convenience: it moved *and* read,
   * so a screen with an effect-driven reader had two readers of one page, and the effect — which
   * re-ran because it read `page.offset` reactively — claimed second and superseded the step every
   * time (`R3-002` of `review-b263118903947f58`, measured as two reads per click).
   */
  stepTo(delta: number): void {
    const next = clampOffset(this.offset + delta * this.#size, this.#size, this.total);
    if (next === this.offset) return;
    this.offset = next;
    this.revision += 1;
  }

  /**
   * Returns the page to the one whose rows are on screen, after a failed read.
   *
   * **`S21`.** With one reader the restore belongs to that reader, and it runs *before* the error is
   * reported so the pager never names a page whose rows were not fetched. It takes no arguments on
   * purpose: the only correct destination is the one the owner recorded when it applied a read, so
   * a caller cannot pass the offset it mistakenly believes preceded the move.
   */
  restore(): void {
    this.offset = this.#applied.offset;
    this.total = this.#applied.total;
  }
}
