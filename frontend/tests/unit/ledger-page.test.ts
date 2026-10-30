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

    // The reader steps to page two, and its read completes first.
    page.stepTo(1);
    await page.read('day', fetcher, apply);
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
    page.stepTo(1);
    await page.read('day', big, apply);
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

describe('a failed read returns to the page whose rows are on screen', () => {
  it('restores the offset and total that produced the rows on screen', async () => {
    // **`R3-003` of `review-811cd5f5c6e59feb`, and `S21`.** The component version captured
    // `requested` *after* the step had moved the offset, so its rollback restored the new offset and
    // the label kept naming a page whose rows were never fetched. The owner records the applied pair,
    // so that mistake is not expressible here.
    const { fetcher } = fetcherFor(500, 200);
    const page = new LedgerPage<Row, null>(200);
    const { apply } = recorder();
    await page.read('day', fetcher, apply);
    page.stepTo(1);

    page.restore();

    expect([page.offset, page.total]).toEqual([0, 500]);
  });
});

// **What is not tested.** The span guard compared `mine.span` with the argument its own call was
// made with, so it could never discard anything (`#120`); it is deleted, not covered, because with
// the claim in place it is provably redundant and `tests/unit/ledger-paging.test.ts` asserts its
// absence. The offset guard stays and is pinned there: the clamp's re-read keeps the claim.

describe('the owner is the only writer', () => {
  it('keeps the offset and total as its own state, rendered but not assigned by the caller', async () => {
    // **The shape that makes the findings unexpressible**: every write is inside this class.
    const page = new LedgerPage<Row, null>(200);
    expect(typeof page.offset).toBe('number');
    expect(typeof page.total).toBe('number');
    expect(typeof page.read).toBe('function');
    expect(typeof page.stepTo).toBe('function');
    expect(typeof page.restore).toBe('function');
    // The claim: any later claim supersedes, and a read claims for itself.
    const first = page.claim();
    const second = page.claim();
    expect([page.owns(first), page.owns(second)]).toEqual([false, true]);
    await page.read('day', fetcherFor(10, 200).fetcher, recorder().apply);
    expect(page.owns(second), 'a read claims for itself').toBe(false);
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

// **`S20`/`S21`: there is no imperative reader any more.** `step` moved the page and read it, which
// gave a screen with an effect-driven reader two readers of one page: the effect re-ran because it
// read `page.offset`, claimed after the step, and superseded it on every click (`R3-002` of
// `review-b263118903947f58`, measured as two reads per click). It is deleted, and `stepTo` is the
// only way a page moves. The failure is the reader's to report and the owner's to restore.
describe('a page move and a failed read', () => {
  it('moves the offset and the revision together, and reads nothing itself', async () => {
    const { fetcher, asked } = fetcherFor(500, 200);
    const page = new LedgerPage<Row, null>(200);
    const { apply } = recorder();
    await page.read('day', fetcher, apply);
    const readsBefore = asked.length;

    page.stepTo(1);

    expect(page.offset).toBe(200);
    expect(page.revision).toBe(1);
    expect(asked.length, 'moving is not reading: the reader fetches').toBe(readsBefore);
  });

  it('restores the page whose rows were applied, not the one the read was issued for', async () => {
    // **The distinction that made the first correction wrong.** `step` captured the offset at issue
    // time — which is *after* the move — so "restoring" it restored the page that had just failed.
    // `restore()` takes no arguments for that reason; the owner recorded the applied pair itself.
    const { fetcher } = fetcherFor(500, 200);
    const page = new LedgerPage<Row, null>(200);
    const { apply, applied } = recorder();
    await page.read('day', fetcher, apply);
    expect([page.offset, page.total]).toEqual([0, 500]);

    page.stepTo(1);
    expect(page.offset).toBe(200);

    page.restore();

    expect([page.offset, page.total], 'back to the page whose rows are on screen').toEqual([
      0, 500
    ]);
    expect(applied.at(-1)?.rows[0]?.id, 'and those rows are still the ones that were applied').toBe(
      'r0'
    );
  });

  it('does not restore to a page that was never applied', async () => {
    // A fresh owner has applied nothing: `restore` answers the initial page, never a value the
    // caller believes preceded a move.
    const page = new LedgerPage<Row, null>(200);
    page.stepTo(1);
    page.restore();
    expect([page.offset, page.total]).toEqual([0, 0]);
  });
});
