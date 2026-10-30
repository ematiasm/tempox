import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

/**
 * A written credit note has to reach the rows, even when the cash figure cannot be re-read.
 *
 * **The defect (#123, `R3-003` of `review-02622a533b5553ec`).** `getCash()` was made unconditional to
 * close `R3-CORR-001` — a stale balance — and it was inserted **before** the refresh, turning it into
 * a gate:
 *
 * ```ts
 * const c = await getCash();
 * cash = c.total;
 * page.refresh();          // ← only reached if getCash resolved
 * ```
 *
 * `getCash()` reads the default account's balance; the day's rows come from `listSales`. A cash
 * failure therefore has nothing to say about the list, but a thrown exception skips the refresh
 * entirely: the note exists on the server, the row on screen never learns about it, and the cashier
 * reads their own write as having done nothing. Before the refactor the list was refreshed first and
 * the balance second, so a cash failure cost a figure the reader can re-read — not the evidence that
 * their write landed.
 *
 * **Against the real component**, not a harness: `R3-CORR-002` of an earlier round was a test that
 * defined its own copy of the rule and asserted that. The alternative — extracting the sequence into
 * its own module — was rejected: the shared thing here is a *sequence of two calls*, and a module
 * that owned it would be a layer whose only purpose is to be testable, while the real component
 * would keep calling the same two functions in its own order.
 */

const api = vi.hoisted(() => ({
  fetchDay: vi.fn(),
  listSales: vi.fn(),
  getSalesSummary: vi.fn(),
  getCash: vi.fn(),
  createCreditNote: vi.fn()
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
    listSales: api.listSales,
    getSalesSummary: api.getSalesSummary,
    getCash: api.getCash,
    createCreditNote: api.createCreditNote
  };
});

// Imported after the mock is registered, so the component receives the doubled client.
const { default: Ledger } = await import('#lib/components/Ledger.svelte');
const { ApiError } = await import('#lib/api.js');

const SALE_ID = 'sale-1';
const ITEM_ID = 'item-1';
const PRODUCT = 'Yerba';

function shopDay() {
  return {
    from: '2026-10-20T03:00:00.000Z',
    to: '2026-10-21T03:00:00.000Z',
    prev: '2026-10-19T03:00:00.000Z',
    next: '2026-10-21T03:00:00.000Z',
    zone: 'America/Argentina/Buenos_Aires'
  };
}

/** One sale, with the lines the refund form reads and the notes it has already produced. */
function sale(creditNotes: unknown[] = []) {
  return {
    id: SALE_ID,
    customer_id: 'walk-in',
    customer_name: 'walk-in',
    total: '25.00',
    payment_method_id: 'pm-1',
    payment_method_name: 'Efectivo',
    created_at: '2026-10-20T12:00:00.000Z',
    items: [
      {
        id: ITEM_ID,
        sale_id: SALE_ID,
        product_id: 'product-1',
        product_name: PRODUCT,
        quantity: 4,
        unit_price: '25.00',
        line_total: '100.00',
        unit: 'unit'
      }
    ],
    credit_notes: creditNotes
  };
}

/** The note the server wrote in these tests: one unit came back. */
function creditNote() {
  return {
    id: 'note-1',
    sale_id: SALE_ID,
    total: '25.00',
    created_at: '2026-10-20T12:05:00.000Z',
    items: [
      {
        id: 'note-item-1',
        credit_note_id: 'note-1',
        sale_item_id: ITEM_ID,
        product_id: 'product-1',
        product_name: PRODUCT,
        quantity: 1,
        unit_price: '25.00',
        line_total: '25.00',
        unit: 'unit'
      }
    ]
  };
}

function summary() {
  return { gross: '100.00', refunded: '0.00', net: '100.00', documents: 1 };
}

async function settle() {
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

describe('a refund whose cash read fails', () => {
  let target: HTMLDivElement;
  let component: Record<string, unknown> | undefined;

  beforeEach(() => {
    Object.values(api).forEach((fn) => fn.mockReset());

    api.fetchDay.mockResolvedValue(shopDay());
    api.getSalesSummary.mockResolvedValue(summary());
    api.getCash.mockResolvedValue({ total: '100.00' });
    api.listSales.mockResolvedValue([sale()]);
    api.createCreditNote.mockResolvedValue(creditNote());

    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(() => {
    if (component) unmount(component);
    component = undefined;
    target.remove();
    document.body.innerHTML = '';
  });

  /** Mounts the ledger, waits for the day, and opens the sale's refund form. */
  async function openRefundForm() {
    component = mount(Ledger, { target, props: {} }) as Record<string, unknown>;
    flushSync();
    await settle();

    const slip = target.querySelector('[data-doc-row]');
    expect(slip, 'the day has a document to refund').not.toBeNull();
    (slip as HTMLElement).click();
    await settle();

    // Arm the two-step confirm: the first press only opens it.
    const arm = [...target.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Anular venta…'
    );
    expect(arm, 'the annul action the open document prints').toBeDefined();
    (arm as HTMLButtonElement).click();
    await settle();
  }

  function confirmAnnul() {
    const confirm = [...target.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Sí, anular'
    );
    expect(confirm, 'the confirm the two-step form prints').toBeDefined();
    (confirm as HTMLButtonElement).click();
  }

  it('still reloads the rows, and says the balance is the thing that is behind', async () => {
    await openRefundForm();

    // From here the day's rows carry the note the server writes, and the cash read fails.
    api.listSales.mockResolvedValue([sale([creditNote()])]);
    api.getCash.mockRejectedValue(new ApiError(500));

    confirmAnnul();
    await settle();

    expect(api.createCreditNote).toHaveBeenCalledTimes(1);

    const noteBlock = target.querySelector('.note-block');
    expect(
      noteBlock,
      'a cash failure must not cost the rows their refresh: the note exists on the server'
    ).not.toBeNull();
    expect(noteBlock?.textContent).toContain('Nota de crédito');

    // The cash figure did not refresh, and the screen says so rather than falling silent — the
    // half of this rule that closed `R3-CORR-001` and must not be traded back for the other.
    expect(target.textContent).toContain('esta pantalla quedó atrás');
    expect(
      target.querySelector('[role="alert"]'),
      'a cash failure is not a failure to read the ledger'
    ).toBeNull();
  });

  it('retires the notice when the cash figure comes back too', async () => {
    await openRefundForm();
    api.listSales.mockResolvedValue([sale([creditNote()])]);

    confirmAnnul();
    await settle();

    expect(target.querySelector('.note-block')).not.toBeNull();
    expect(target.textContent, 'nothing is stale once both reads succeeded').not.toContain(
      'esta pantalla quedó atrás'
    );
  });
});

/**
 * **`R3-004` of `review-b263118903947f58`: a notice is retired only once the reads returned** (`S23`).
 *
 * The refund path called `refundStale.retireAll()` right after `page.refresh()`, before either the
 * reload or the cash read had come back. The reachable case is the one this file already owns: the
 * rows arrive and the **cash** read fails. The old order had already erased every notice by then, so
 * the screen said nothing while the balance was stale — and the `mark` that follows had nothing left
 * to preserve. `screenTrust.ts` states the rule ("retire when the figures *are* current"); this was
 * the ordering breaking it, not the rule.
 *
 * A reload that fails outright is not the discriminating case: `error` replaces the ledger region,
 * so no notice is visible either way. Recorded here so the next reader does not write that test.
 */
describe('a refund whose cash read fails after the rows arrived', () => {
  let target: HTMLDivElement;
  let component: Record<string, unknown> | undefined;

  beforeEach(() => {
    Object.values(api).forEach((fn) => fn.mockReset());
    api.fetchDay.mockResolvedValue(shopDay());
    api.getSalesSummary.mockResolvedValue(summary());
    api.getCash.mockResolvedValue({ total: '100.00' });
    api.listSales.mockResolvedValue([sale()]);
    api.createCreditNote.mockResolvedValue(creditNote());
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(() => {
    if (component) unmount(component);
    component = undefined;
    target.remove();
    document.body.innerHTML = '';
  });

  it('reports the stale balance rather than erasing every notice first', async () => {
    component = mount(Ledger, { target, props: {} }) as Record<string, unknown>;
    flushSync();
    await settle();

    const slip = target.querySelector('[data-doc-row]');
    (slip as HTMLElement).click();
    await settle();
    const arm = [...target.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Anular venta…'
    );
    (arm as HTMLButtonElement).click();
    await settle();

    // The rows come back carrying the note; the balance does not.
    api.listSales.mockResolvedValue([sale([creditNote()])]);
    api.getCash.mockRejectedValue(new ApiError(500));
    const confirm = [...target.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === 'Sí, anular'
    );
    (confirm as HTMLButtonElement).click();
    await settle();

    expect(api.createCreditNote).toHaveBeenCalledTimes(1);
    expect(target.querySelector('.note-block'), 'the note reached its row').not.toBeNull();
    // **This assertion does not discriminate the two orderings, and that is recorded honestly.**
    // With the old order `retireAll()` also ran, but `mark()` then re-inserted the message for this
    // row, so the notice appeared either way. The ordering is still wrong — a notice about a
    // *different* row would be lost, and `R3-004` is right about that — but no reachable UI state
    // exposes it, so the fix is pinned by intention and recorded as debt in `L31` rather than
    // dressed up as a test that bites.
    expect(target.textContent).toContain('esta pantalla quedó atrás');
  });
});

/**
 * **`S21` for the sales ledger: a failed read does not leave the pager ahead of the rows.**
 *
 * Not a `review-b263118903947f58` finding on its own — it is the transfer of the purchases defect to
 * the screen that shares the owner. Without it, deleting `page.restore()` from the sales reader
 * changed no test's outcome (measured), which is the definition of a rule resting on nothing.
 */
describe('a sales page step whose read fails', () => {
  let target: HTMLDivElement;
  let component: Record<string, unknown> | undefined;

  beforeEach(() => {
    Object.values(api).forEach((fn) => fn.mockReset());
    api.fetchDay.mockResolvedValue(shopDay());
    api.getSalesSummary.mockResolvedValue({ ...summary(), documents: 500 });
    api.getCash.mockResolvedValue({ total: '100.00' });
    api.listSales.mockResolvedValue([sale()]);
    api.createCreditNote.mockResolvedValue(creditNote());
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(() => {
    if (component) unmount(component);
    component = undefined;
    target.remove();
    document.body.innerHTML = '';
  });

  it('keeps the pager on the page whose rows loaded', async () => {
    component = mount(Ledger, { target, props: {} }) as Record<string, unknown>;
    flushSync();
    await settle();

    const label = () => target.querySelector('.page-of')?.textContent?.trim() ?? '';
    expect(label(), 'the page pager only prints when the period is partial').toContain('Página 1');

    // Page two fails.
    api.listSales.mockImplementation(async (params: { offset?: number }) => {
      if ((params.offset ?? 0) >= 50) throw new ApiError(503);
      return [sale()];
    });
    const older = [...target.querySelectorAll('.footings .pager button')].find((b) =>
      (b.textContent ?? '').includes('Anterior')
    );
    expect(older, 'the page pager the sales footings print').toBeDefined();
    (older as HTMLButtonElement).click();
    await settle();

    expect(label(), 'the label must not name a page whose rows were never fetched').toContain(
      'Página 1'
    );
    expect(target.querySelector('[role="alert"]')).not.toBeNull();
  });
});

/**
 * **`R4-001` of `review-4d05dcfca7ad3826`: one writer for the balance.**
 *
 * The finding measured two `getCash()` calls racing during a refund — the refund's own and the
 * effect's, started by `page.refresh()` — where a later failure blanked a figure that had just been
 * read. The correction is `S20` applied to the figure: the refund reads nothing, the effect is the
 * only cash reader, and a failure marks the screen as behind instead of silently blanking it.
 *
 * Pinned as a count, because that is what distinguishes "one writer" from "two writers that
 * happen to agree": exactly one `getCash` per effect run, and none from the refund path.
 */
describe('one writer for the balance', () => {
  let target: HTMLDivElement;
  let component: Record<string, unknown> | undefined;

  beforeEach(() => {
    Object.values(api).forEach((fn) => fn.mockReset());
    api.fetchDay.mockResolvedValue(shopDay());
    api.getSalesSummary.mockResolvedValue(summary());
    api.listSales.mockResolvedValue([sale()]);
    api.createCreditNote.mockResolvedValue(creditNote());
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(() => {
    if (component) unmount(component);
    component = undefined;
    target.remove();
    document.body.innerHTML = '';
  });

  it('reads the balance once per reload, from the effect alone', async () => {
    let cashReads = 0;
    api.getCash.mockImplementation(async () => {
      cashReads += 1;
      return { total: '100.00' };
    });

    component = mount(Ledger, { target, props: {} }) as Record<string, unknown>;
    flushSync();
    await settle();
    const afterMount = cashReads;
    expect(afterMount, 'the mount reads it once').toBe(1);

    (target.querySelector('[data-doc-row]') as HTMLElement).click();
    await settle();
    (
      [...target.querySelectorAll('button')].find(
        (b) => b.textContent?.trim() === 'Anular venta…'
      ) as HTMLButtonElement
    ).click();
    await settle();

    api.listSales.mockResolvedValue([sale([creditNote()])]);
    (
      [...target.querySelectorAll('button')].find(
        (b) => b.textContent?.trim() === 'Sí, anular'
      ) as HTMLButtonElement
    ).click();
    await settle();

    expect(
      cashReads - afterMount,
      'the refund reloads through the one reader: one more read, not two'
    ).toBe(1);
    const balance = () => target.querySelector('.footings .fig .value')?.textContent?.trim() ?? '';
    expect(balance()).toContain('100');
  });
});

/**
 * **`R3-002` of `review-a88d442d3e677eb8`: a recorded note whose reload fails says the note landed.**
 *
 * Moving the read out of the refund path made the effect the one reader; the place that reported
 * "the note exists, the screen is behind" went with it. The failed reload raised the generic ledger
 * error, and that sentence — *could not read the book* — erases the only fact the cashier needs and
 * invites a second note on top of the first. The heading must say what actually happened.
 */
describe('a recorded note whose reload fails', () => {
  let target: HTMLDivElement;
  let component: Record<string, unknown> | undefined;

  beforeEach(() => {
    Object.values(api).forEach((fn) => fn.mockReset());
    api.fetchDay.mockResolvedValue(shopDay());
    api.getSalesSummary.mockResolvedValue(summary());
    api.getCash.mockResolvedValue({ total: '100.00' });
    api.createCreditNote.mockResolvedValue(creditNote());
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(() => {
    if (component) unmount(component);
    component = undefined;
    target.remove();
    document.body.innerHTML = '';
  });

  it('says the note was recorded rather than that the book could not be read', async () => {
    api.listSales.mockResolvedValue([sale()]);
    component = mount(Ledger, { target, props: {} }) as Record<string, unknown>;
    flushSync();
    await settle();

    (target.querySelector('[data-doc-row]') as HTMLElement).click();
    await settle();
    (
      [...target.querySelectorAll('button')].find(
        (b) => b.textContent?.trim() === 'Anular venta…'
      ) as HTMLButtonElement
    ).click();
    await settle();

    // The note lands; its reload does not.
    api.listSales.mockRejectedValue(new ApiError(503));
    (
      [...target.querySelectorAll('button')].find(
        (b) => b.textContent?.trim() === 'Sí, anular'
      ) as HTMLButtonElement
    ).click();
    await settle();

    expect(api.createCreditNote).toHaveBeenCalledTimes(1);
    const alert = target.querySelector('[role="alert"]')?.textContent ?? '';
    expect(alert, 'the note is a fact and the message says it').toContain(
      'La devolución quedó registrada'
    );
    expect(alert, 'and not "could not read the book", which invites a second note').not.toContain(
      'No se pudo leer el libro'
    );
  });
});
