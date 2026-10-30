import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

/**
 * A load that has been superseded may not publish its failure.
 *
 * **The defect (#122, `R3-002` of `review-02622a533b5553ec`).** Moving the purchases screen onto the
 * shared page owner deleted the `isStale` predicate the loader used to take, so the effect became
 *
 * ```ts
 * readDay(at).catch((e) => (error = describe(e))).finally(() => (loading = false));
 * ```
 *
 * with no guard at all. A slow day load that *fails* therefore wrote its error — and cleared
 * `loading` — after a newer load had already started or succeeded. The reader's symptom is the
 * nastiest kind: a correct day on screen with an error about a different day printed above it, which
 * is exactly what `screenTrust.ts` exists to prevent.
 *
 * The other two component tests in this directory mount a harness that reproduces a shape. This one
 * mounts **the real component** against a mocked client instead, because the question is about the
 * component's own effect and a harness would only prove that a copy of the defect misbehaves.
 *
 * Reproduced deterministically, with no browser race: hold the failing day's `listPurchases` promise,
 * let the newer day resolve, and only then release the failure. Releasing it *last* is what makes an
 * unguarded write visible — the same trap the `ledgerPage` suite documents.
 */

const api = vi.hoisted(() => ({
  fetchDay: vi.fn(),
  listPurchases: vi.fn(),
  getPurchasesSummary: vi.fn(),
  listSuppliers: vi.fn(),
  createSupplierReturn: vi.fn()
}));

vi.mock('#lib/api.js', () => {
  class ApiError extends Error {
    readonly status: number;

    constructor(status: number, message?: string) {
      super(message ?? `request failed: ${status}`);
      this.name = 'ApiError';
      this.status = status;
    }
  }
  return {
    ApiError,
    fetchDay: api.fetchDay,
    listPurchases: api.listPurchases,
    getPurchasesSummary: api.getPurchasesSummary,
    listSuppliers: api.listSuppliers,
    createSupplierReturn: api.createSupplierReturn
  };
});

// Imported after the mock is registered, so the component receives the doubled client.
const { default: PurchaseLedger } = await import('#lib/components/PurchaseLedger.svelte');
const { ApiError } = await import('#lib/api.js');

/** One shop day, on a UTC calendar, so a step is exactly the neighbouring `DayRange`. */
function dayAt(instant: number) {
  const day = Math.floor(instant / 86_400_000) * 86_400_000;
  return {
    from: new Date(day).toISOString(),
    to: new Date(day + 86_400_000).toISOString(),
    prev: new Date(day - 86_400_000).toISOString(),
    next: new Date(day + 86_400_000).toISOString(),
    zone: 'UTC'
  };
}

function purchase(id: string, total: string) {
  return {
    purchase: {
      id,
      supplier_id: 'one-off-supplier',
      total,
      payment_method_id: 'pm-1',
      payment_method_name: 'Efectivo',
      created_at: '2026-10-20T12:00:00.000Z'
    },
    items: [],
    supplier_returns: []
  };
}

/** A promise whose settlement the test decides, so the order of the answers is the assertion. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Let the pending microtasks and effects run, then sync the DOM. */
async function settle() {
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

describe('a superseded day load', () => {
  let target: HTMLDivElement;
  let component: Record<string, unknown> | undefined;

  beforeEach(() => {
    api.fetchDay.mockReset();
    api.listPurchases.mockReset();
    api.getPurchasesSummary.mockReset();
    api.listSuppliers.mockReset();

    api.fetchDay.mockImplementation(async (at: Date) => dayAt(at.getTime()));
    api.listSuppliers.mockResolvedValue([]);
    api.getPurchasesSummary.mockResolvedValue({
      gross: '0.00',
      returned: '0.00',
      net: '0.00',
      documents: 0
    });

    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(() => {
    if (component) unmount(component);
    component = undefined;
    target.remove();
    document.body.innerHTML = '';
  });

  it('does not publish its error over the day that replaced it', async () => {
    // Today's load: succeeds, so the pager's day buttons become enabled.
    api.listPurchases.mockResolvedValue([purchase('today', '10.00')]);
    component = mount(PurchaseLedger, { target, props: {} }) as Record<string, unknown>;
    flushSync();
    await settle();
    expect(target.querySelectorAll('[data-doc-row]')).toHaveLength(1);

    // The day before: its list request is held, so this load is still in flight.
    const held = deferred<unknown>();
    let requestedDay: number | null = null;
    api.listPurchases.mockImplementation(async (params: { from: string }) => {
      const which = Date.parse(params.from);
      if (which === Date.parse(dayAt(Date.now()).from) - 86_400_000) {
        requestedDay = which;
        return held.promise;
      }
      return [purchase('newest', '20.00')];
    });

    const older = target.querySelectorAll('button');
    // The two day buttons are the ones the header prints; find them by their word.
    const olderButton = [...older].find((b) => b.textContent?.trim() === 'Anterior');
    expect(olderButton, 'the day button the header prints').toBeDefined();
    olderButton!.click();
    await settle();
    expect(requestedDay, 'the previous day was requested and left in flight').not.toBeNull();

    // Back to the newer day, which resolves while the previous one is still held.
    const newerButton = [...target.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Siguiente'
    );
    newerButton!.click();
    await settle();
    expect(
      [...target.querySelectorAll('[data-doc-row]')].map((row) => row.getAttribute('data-doc-row')),
      'the newer day is the one on screen'
    ).toEqual(['newest']);

    // Now the superseded load fails. Its error describes a day nobody is looking at.
    held.reject(new ApiError(500));
    await settle();

    expect(
      target.querySelector('[role="alert"]'),
      'a superseded load must not print an error over the day that replaced it'
    ).toBeNull();
    expect(
      [...target.querySelectorAll('[data-doc-row]')].map((row) => row.getAttribute('data-doc-row')),
      'the day on screen is untouched'
    ).toEqual(['newest']);
  });

  /**
   * The other half of the same guard: the completion flag.
   *
   * The old loader gated `error` **and** `loading` behind the same predicate. Without the second
   * half, a superseded load that fails clears `loading` while a newer read is still in flight, so
   * the screen stops saying it is loading and presents the previous day's rows as current — the
   * reader cannot tell that what they are looking at is about to be replaced.
   */
  it('does not clear the loading state while a newer read is still in flight', async () => {
    // A multi-page period, so the footer pager exists at all (`R3-001` of
    // `review-b263118903947f58`: the first version clicked the header's day buttons, which print the
    // same words and change the day instead of the page, so the held read it built was never used).
    api.getPurchasesSummary.mockResolvedValue({
      gross: '0.00',
      returned: '0.00',
      net: '0.00',
      documents: 500
    });
    api.listPurchases.mockResolvedValue([purchase('today', '10.00')]);

    component = mount(PurchaseLedger, { target, props: {} }) as Record<string, unknown>;
    flushSync();
    await settle();

    // Page two is held: that read is in flight and owns the screen.
    const held = deferred<unknown>();
    api.listPurchases.mockImplementation(async (params: { offset?: number }) =>
      (params.offset ?? 0) === 200 ? held.promise : [purchase('today', '10.00')]
    );

    const older = [...target.querySelectorAll('.footings .pager button')].find((b) =>
      (b.textContent ?? '').includes('Anterior')
    );
    expect(older, 'the footer pager the ledger prints').toBeDefined();
    (older as HTMLButtonElement).click();
    await settle();
    expect(target.textContent, 'the page-two read is in flight').toContain('Cargando…');

    // A second read — the reader moving again — supersedes it, and the superseded failure must not
    // report the screen as finished while the newer one is still pending.
    const second = deferred<unknown>();
    api.listPurchases.mockImplementation(async () => second.promise);
    const newer = [...target.querySelectorAll('.footings .pager button')].find((b) =>
      (b.textContent ?? '').includes('Siguiente')
    );
    (newer as HTMLButtonElement).click();
    await settle();
    expect(target.textContent).toContain('Cargando…');

    held.reject(new ApiError(500));
    await settle();

    expect(
      target.textContent,
      'a superseded load must not report the screen as finished while a newer read is pending'
    ).toContain('Cargando…');
    expect(target.querySelector('[role="alert"]')).toBeNull();

    second.resolve([purchase('pageone', '10.00')]);
    await settle();
  });
});

/**
 * **`R3-002` of `review-b263118903947f58`: one click must cause one read** (`S20`).
 *
 * Measured with a probe before this test existed: a single press of the footer pager produced **two**
 * reads of offset 200. `stepPage` called `page.step`, which moves the offset and reads; but the
 * effect reads `page.offset` through `readDay`, so moving the offset re-ran the effect too, and its
 * read claimed *after* the step's — superseding the step on every click. The `owns` gate then made
 * the step's rollback unreachable rather than conditional.
 *
 * The count is the assertion because it is the only thing that distinguishes the two designs: a
 * token in either reader cannot gate the other, so "gated" and "never reached" look identical from
 * the outside. One reader is the fix; this test is what says so.
 */
describe('one page, one reader', () => {
  let target: HTMLDivElement;
  let component: Record<string, unknown> | undefined;

  beforeEach(() => {
    Object.values(api).forEach((fn) => fn.mockReset());
    api.fetchDay.mockImplementation(async (at: Date) => dayAt(at.getTime()));
    api.listSuppliers.mockResolvedValue([]);
    api.getPurchasesSummary.mockResolvedValue({
      gross: '0.00',
      returned: '0.00',
      net: '0.00',
      documents: 500
    });
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(() => {
    if (component) unmount(component);
    component = undefined;
    target.remove();
    document.body.innerHTML = '';
  });

  /** A purchase at the given offset. */
  function pageRows(offset: number) {
    return [purchase(`p${offset}`, '1.00')];
  }

  /** The footer pager's "Older" button — not the header's day button, which says the same word. */
  function pagerOlder(): HTMLButtonElement {
    const button = [...target.querySelectorAll('.footings .pager button')].find((b) =>
      (b.textContent ?? '').includes('Anterior')
    );
    expect(button, 'the footer pager the ledger prints').toBeDefined();
    return button as HTMLButtonElement;
  }

  function pagerLabel(): string {
    return target.querySelector('.page-of')?.textContent?.trim() ?? '';
  }

  function rowIds(): (string | null)[] {
    return [...target.querySelectorAll('[data-doc-row]')].map((r) =>
      r.getAttribute('data-doc-row')
    );
  }

  it('reads the page once per click, not twice', async () => {
    const asked: number[] = [];
    api.listPurchases.mockImplementation(async (params: { offset?: number }) => {
      asked.push(params.offset ?? -1);
      return pageRows(params.offset ?? 0);
    });

    component = mount(PurchaseLedger, { target, props: {} }) as Record<string, unknown>;
    flushSync();
    await settle();
    const before = asked.length;

    pagerOlder().click();
    await settle();

    expect(
      asked.slice(before),
      `a click on the pager must issue exactly one read; asked: ${JSON.stringify(asked)}`
    ).toEqual([200]);
    expect(pagerLabel()).toContain('Página 2');
    expect(rowIds()).toEqual(['p200']);
  });

  /**
   * **`R4-001`/`R3-003`: a failed read leaves the page where the rows are** (`S21`).
   *
   * With two readers the failing read could not restore anything: the step's rollback was gated out
   * by a claim that always belonged to the effect, so the pager advanced to a page whose rows were
   * never fetched and the table below it emptied.
   */
  it('keeps the pager on the page whose rows loaded when the next one fails', async () => {
    api.listPurchases.mockImplementation(async (params: { offset?: number }) => {
      if ((params.offset ?? 0) === 200) throw new ApiError(500);
      return pageRows(params.offset ?? 0);
    });

    component = mount(PurchaseLedger, { target, props: {} }) as Record<string, unknown>;
    flushSync();
    await settle();
    expect(rowIds()).toEqual(['p0']);
    expect(pagerLabel()).toContain('Página 1');

    pagerOlder().click();
    await settle();

    expect(pagerLabel(), 'the pager must not name a page whose rows never loaded').toContain(
      'Página 1'
    );
    expect(target.querySelector('[role="alert"]'), 'and the failure is reported').not.toBeNull();

    // The restore is not cosmetic: the retry the alert offers re-reads the page the reader is on,
    // and those rows come back — only possible because the offset went back (`S21`).
    api.listPurchases.mockImplementation(async (params: { offset?: number }) =>
      pageRows(params.offset ?? 0)
    );
    const retry = [...target.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Reintentar'
    );
    expect(retry, 'the alert offers the retry it always has').toBeDefined();
    (retry as HTMLButtonElement).click();
    await settle();
    expect(rowIds()).toEqual(['p0']);
    expect(pagerLabel()).toContain('Página 1');
  });
});
