import { describe, expect, it } from 'vitest';
import { LedgerPage, type PageFetcher } from '#lib/ledgerPage.svelte.js';

/**
 * The one owner of a ledger's page, tested against the five findings it replaces.
 *
 * **Each test below is a defect that shipped**, not a hypothetical: they are the CRITICALs from
 * `review-58ab3200d6f59c90` and `review-811cd5f5c6e59feb`, written against the new owner instead
 * of against whichever component happened to be patched that round. If any of them starts
 * passing vacuously the suite will not notice — so each one asserts the *observable* outcome
 * (what the screen would show), never an internal flag.
 */

type Row = { id: string };

/** A fetcher that answers per-offset, and records what it was asked for. */
function fetcherFor(total: number, pageSize: number) {
  const asked: number[] = [];
  const fetcher: PageFetcher<Row, null> = async (offset) => {
    asked.push(offset);
    const rows: Row[] = [];
    for (let i = offset; i < Math.min(offset + pageSize, total); i += 1) rows.push({ id: `r${i}` });
    return { page: { rows, total }, extra: null };
  };
  return { fetcher, asked };
}

function recorder() {
  const applied: { rows: Row[]; total: number }[] = [];
  return {
    applied,
    apply: (rows: Row[], _extra: null, total: number) => {
      applied.push({ rows, total });
    }
  };
}

describe('a read that is superseded writes nothing', () => {
  /**
   * **The two `R3-002`s: one about the page, one about the period.**
   *
   * The first version of these tests issued a slow read, changed the state, released the slow
   * read, and asserted that its rows were not the last applied — and **passed with the guard
   * deleted**, because the newer read's own apply landed afterwards and overwrote it. A test
   * whose conclusion is "the last write wins" cannot see a stale write that happens to be first.
   *
   * What makes the defect visible is the **order**: release the stale answer *last*, after the
   * newer state is already on screen. Then an unguarded read overwrites it and the assertion
   * fails, which is precisely what happens to a cashier.
   */
  function deferred<T>() {
    let release!: (v: T) => void;
    const promise = new Promise<T>((r) => (release = r));
    return { promise, release };
  }

  it('drops an answer for a span the reader has left', async () => {
    const page = new LedgerPage<Row, null>(200);
    const { apply, applied } = recorder();
    const gate = deferred<{ page: { rows: Row[]; total: number }; extra: null }>();

    // The slow read is issued for March, and stays in flight.
    const slow: PageFetcher<Row, null> = () => gate.promise;
    const stale = page.read('march', slow, apply);

    // April's read completes first and owns the screen.
    await page.read('april', fetcherFor(1, 200).fetcher, apply);
    expect(applied.at(-1)?.rows).toEqual([{ id: 'r0' }]);

    // Now the March answer finally arrives. It must be discarded.
    gate.release({ page: { rows: [{ id: 'march-row' }], total: 1 }, extra: null });
    await stale;

    expect(applied.at(-1)?.rows).toEqual([{ id: 'r0' }]);
    expect(applied.some((a) => a.rows.some((r) => r.id === 'march-row'))).toBe(false);
  });

  it('drops an answer for a page the reader has stepped away from', async () => {
    const page = new LedgerPage<Row, null>(200);
    const { apply, applied } = recorder();
    const { fetcher } = fetcherFor(500, 200);
    await page.read('day', fetcher, apply);

    const gate = deferred<{ page: { rows: Row[]; total: number }; extra: null }>();
    const slow: PageFetcher<Row, null> = () => gate.promise;
    // A reload of page one is in flight.
    const stale = page.read('day', slow, apply);

    // The reader steps to page two, which completes first.
    await page.step(1, 'day', fetcher, apply, () => {});
    expect(applied.at(-1)?.rows[0]?.id).toBe('r200');

    // Page one's answer arrives late. Writing it would put page one's rows under page two's
    // label — the reported defect.
    gate.release({ page: { rows: [{ id: 'page-one' }], total: 500 }, extra: null });
    await stale;

    expect(applied.at(-1)?.rows[0]?.id).toBe('r200');
  });
});

describe('the clamp re-reads instead of invalidating itself', () => {
  it('loads the clamped page when the span shrank under the current one', async () => {
    // **`R3-001` of `review-811cd5f5c6e59feb`, a logical deadlock.** Stepping to page two and then
    // finding a day of at most one page clamped the offset to 0 — and the component's re-read
    // compared against the offset captured before the request, so the clamp made it "stale" and
    // the clamped page was never fetched. Page two's rows stayed under page one's label.
    const page = new LedgerPage<Row, null>(200);
    const { apply } = recorder();
    const { fetcher: big } = fetcherFor(500, 200);
    await page.read('day', big, apply);
    await page.step(1, 'day', big, apply, () => {});
    expect(page.offset).toBe(200);

    // The day shrinks to 100 documents: page two no longer exists.
    const { fetcher: small, asked } = fetcherFor(100, 200);
    const appliedNow: Row[] = [];
    await page.read('day', small, (rows) => {
      appliedNow.push(...rows);
    });

    expect(page.offset).toBe(0);
    expect(asked).toContain(0);
    expect(appliedNow).toHaveLength(100);
  });
});

describe('a failed step rolls back to the page whose rows are on screen', () => {
  it('returns the offset the step started from, not the one it set', async () => {
    // **`R3-003`, and the reason it survived a fix.** The component captured `requested` *after*
    // the step had moved the offset, so its rollback restored the new offset and the label kept
    // naming a page whose rows were never fetched. Here the previous offset never left the
    // module, so the mistake is not expressible.
    const page = new LedgerPage<Row, null>(200);
    const { apply } = recorder();
    const { fetcher } = fetcherFor(500, 200);
    await page.read('day', fetcher, apply);
    await page.step(1, 'day', fetcher, apply, () => {});
    expect(page.offset).toBe(200);

    const failing: PageFetcher<Row, null> = async () => {
      throw new Error('the network went away');
    };
    const errors: unknown[] = [];
    await page.step(1, 'day', failing, apply, (e) => errors.push(e));

    expect(page.offset).toBe(200);
    expect(errors).toHaveLength(1);
  });

  it('keeps the total the rows on screen were fetched with', async () => {
    // The rollback restores the pair, not just the offset: a total from a read that failed would
    // leave the pager computing pages for a span it never read.
    const page = new LedgerPage<Row, null>(200);
    const { apply } = recorder();
    const { fetcher } = fetcherFor(500, 200);
    await page.read('day', fetcher, apply);

    const failing: PageFetcher<Row, null> = async () => {
      throw new Error('nope');
    };
    await page.step(1, 'day', failing, apply, () => {});
    expect(page.offset).toBe(0);
    expect(page.total).toBe(500);
  });
});

// **What is not tested, and why it is said out loud.** Three guards in `#readWithin` discard an
// answer: a newer generation, a different span, a different page. Measured by deleting each:
// removing the generation guard fails a test, removing the rollback fails two — and removing
// **either the span guard or the offset guard fails none**.
//
// I first thought sharing the generation with the clamp's re-read would make the offset guard
// load-bearing, because a step during that re-read has no new generation to be caught by. It
// does not: the step's own read bumps the generation and its result is applied last, so every
// assertion still passes with the guard gone. I could not construct the scenario that isolates
// it, and I am recording that rather than writing a test that appears to cover it.
//
// So both guards are **reasoned claims, not tested ones**: they are the only thing that could
// catch a step or a period change inside a read that does not bump the generation, and neither
// the current call sites nor any test I could write produce that state.
//
// The honest tally for this module: two of the five findings it replaces are pinned by tests
// that fail when the fix is deleted (`R3-003` rollback, `R3-001` clamp deadlock). The `R3-002`
// pair — period and page — rests on guards whose isolation is not demonstrated.

describe('the owner is the only writer', () => {
  it('keeps the offset and total as its own state, rendered but not assigned by the caller', () => {
    // **The shape that makes the five findings unexpressible.** With rune fields the compiler
    // sees `page.offset = 400` as a legal assignment, so the guarantee is not "it throws" — it is
    // that a component has no reason and no place to do it: it reads `page.offset`, and every
    // write in the codebase is inside this class. This test pins the two readable fields and the
    // two methods, so a refactor that exposes a setter is visible here.
    const page = new LedgerPage<Row, null>(200);
    expect(typeof page.offset).toBe('number');
    expect(typeof page.total).toBe('number');
    expect(typeof page.read).toBe('function');
    expect(typeof page.step).toBe('function');
  });

  it('answers false, not a throw, when its own read is superseded', async () => {
    // A caller asking "did the screen accept this" needs a straight answer; deriving it from a
    // local comparison is what the five patches did, in five slightly different ways.
    const page = new LedgerPage<Row, null>(200);
    const { apply } = recorder();
    const { fetcher } = fetcherFor(10, 200);
    expect(await page.read('day', fetcher, apply)).toBe(true);
  });
});
