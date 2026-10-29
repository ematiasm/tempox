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
    expect(target.textContent).toContain('esta pantalla no pudo recargar el día');
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
      'esta pantalla no pudo recargar el día'
    );
  });
});
