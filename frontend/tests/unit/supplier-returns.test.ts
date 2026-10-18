import { describe, expect, it } from 'vitest';
import {
  isFullyReturned,
  parseReturnQuantity,
  planReturn,
  returnableOf,
  returnedByLine
} from '#lib/supplierReturns.js';
import type { PurchaseItem, SupplierReturnWithItems } from '#lib/api.js';

/**
 * What is left to return on a purchase, and which return a form would send (S6).
 *
 * This arithmetic exists in the sales ledger only inside its component, where no
 * unit test can reach it — and the credit note measured exactly what that costs:
 * `[{line, 6}, {line, 6}]` against a line of 10 returned 12, because each entry
 * was checked on its own. The server now guards that, but the screen that offers
 * the quantity should not propose a plan the server must refuse.
 *
 * These functions are pure and take the server's own arrays, so the component
 * derives what it displays from the record rather than from a local tally that
 * drifts the moment a return lands in another tab.
 */

function line(id: string, quantity: number): PurchaseItem {
  return {
    id,
    purchase_id: 'p1',
    product_id: `prod-${id}`,
    product_name: `Product ${id}`,
    quantity,
    unit_price: '10.00',
    line_total: `${(quantity * 10).toFixed(2)}`,
    unit: 'unit'
  };
}

function supplierReturn(id: string, items: { purchase_item_id: string; quantity: number }[]) {
  return {
    return: { id, purchase_id: 'p1', total: '0.00', created_at: '2026-10-18T12:00:00Z' },
    items: items.map((item) => ({
      id: `${id}-${item.purchase_item_id}`,
      supplier_return_id: id,
      purchase_item_id: item.purchase_item_id,
      product_id: `prod-${item.purchase_item_id}`,
      product_name: `Product ${item.purchase_item_id}`,
      quantity: item.quantity,
      unit_price: '10.00',
      line_total: '0.00',
      unit: 'unit' as const
    }))
  } satisfies SupplierReturnWithItems;
}

describe('returnedByLine', () => {
  it('sums several returns of the same purchase line', () => {
    // Partial returns are the normal case: goods arrive wrong in two batches and
    // go back in two documents. What went back is the sum, never the last one.
    const returned = returnedByLine([
      supplierReturn('r1', [{ purchase_item_id: 'i1', quantity: 3 }]),
      supplierReturn('r2', [{ purchase_item_id: 'i1', quantity: 2 }])
    ]);
    expect(returned.get('i1')).toBe(5);
  });

  it('keys each line separately, so returning one never moves another', () => {
    const returned = returnedByLine([
      supplierReturn('r1', [
        { purchase_item_id: 'i1', quantity: 4 },
        { purchase_item_id: 'i2', quantity: 1 }
      ])
    ]);
    expect(returned.get('i1')).toBe(4);
    expect(returned.get('i2')).toBe(1);
  });

  it('is empty when nothing has gone back, which is what a fresh purchase is', () => {
    expect(returnedByLine([]).size).toBe(0);
  });
});

describe('returnableOf', () => {
  it('is what was bought minus what came back', () => {
    const returned = returnedByLine([
      supplierReturn('r1', [{ purchase_item_id: 'i1', quantity: 4 }])
    ]);
    expect(returnableOf('i1', 10, returned)).toBe(6);
  });

  it('never goes negative, even if the record somehow holds more than was bought', () => {
    // The server prevents this, so this is defensive rather than expected. A
    // negative "left" would render as a negative quantity the operator could
    // then send, so it is clamped instead of trusted.
    const returned = returnedByLine([
      supplierReturn('r1', [{ purchase_item_id: 'i1', quantity: 11 }])
    ]);
    expect(returnableOf('i1', 10, returned)).toBe(0);
  });

  it('treats a line with no returns as fully returnable', () => {
    expect(returnableOf('i1', 10, new Map())).toBe(10);
  });
});

describe('isFullyReturned', () => {
  it('is true only when every line has come back in full', () => {
    const items = [line('i1', 10), line('i2', 4)];
    const returns = [
      supplierReturn('r1', [
        { purchase_item_id: 'i1', quantity: 10 },
        { purchase_item_id: 'i2', quantity: 4 }
      ])
    ];
    expect(isFullyReturned(items, returns)).toBe(true);
  });

  it('is false when one line has a single unit left', () => {
    // A partial return is not a reversal. The purchase still stands, so the
    // screen must keep offering the action.
    const items = [line('i1', 10), line('i2', 4)];
    const returns = [
      supplierReturn('r1', [
        { purchase_item_id: 'i1', quantity: 10 },
        { purchase_item_id: 'i2', quantity: 3 }
      ])
    ];
    expect(isFullyReturned(items, returns)).toBe(false);
  });

  it('is false for a purchase with no lines at all', () => {
    // Vacuously every line is returned, but a document with no lines is not a
    // document that went back; offering nothing is the honest answer.
    expect(isFullyReturned([], [])).toBe(false);
  });
});

describe('planReturn', () => {
  it('plans every line that still has something left', () => {
    const items = [line('i1', 10), line('i2', 4)];
    const returned = new Map<string, number>();
    const requested = new Map<string, number | null>([
      ['i1', 6],
      ['i2', 4]
    ]);
    expect(planReturn(items, returned, requested)).toEqual([
      { purchase_item_id: 'i1', quantity: 6 },
      { purchase_item_id: 'i2', quantity: 4 }
    ]);
  });

  it('skips a line that is already fully returned instead of failing the plan', () => {
    // The form prefills every line, and a fully returned line prefills to zero.
    // Zero is not an error to report: it is a line with nothing to offer.
    const items = [line('i1', 10), line('i2', 4)];
    const returned = returnedByLine([
      supplierReturn('r1', [{ purchase_item_id: 'i2', quantity: 4 }])
    ]);
    const requested = new Map<string, number | null>([
      ['i1', 5],
      ['i2', 0]
    ]);
    expect(planReturn(items, returned, requested)).toEqual([
      { purchase_item_id: 'i1', quantity: 5 }
    ]);
  });

  it('refuses a plan asking for more than a line has left', () => {
    // The server answers 409 here. Catching it on this side is not a second
    // authority — it is refusing to send a request whose refusal is known.
    const items = [line('i1', 10)];
    const returned = returnedByLine([
      supplierReturn('r1', [{ purchase_item_id: 'i1', quantity: 4 }])
    ]);
    expect(planReturn(items, returned, new Map([['i1', 7]]))).toBeNull();
  });

  it('accepts a plan asking for exactly what is left, the boundary the server allows', () => {
    const items = [line('i1', 10)];
    const returned = returnedByLine([
      supplierReturn('r1', [{ purchase_item_id: 'i1', quantity: 4 }])
    ]);
    expect(planReturn(items, returned, new Map([['i1', 6]]))).toEqual([
      { purchase_item_id: 'i1', quantity: 6 }
    ]);
  });

  it('refuses a plan with a quantity that is not a positive integer', () => {
    const items = [line('i1', 10)];
    expect(planReturn(items, new Map(), new Map([['i1', null]]))).toBeNull();
    expect(planReturn(items, new Map(), new Map([['i1', 0]]))).toBeNull();
    expect(planReturn(items, new Map(), new Map([['i1', -1]]))).toBeNull();
    expect(planReturn(items, new Map(), new Map([['i1', 1.5]]))).toBeNull();
  });

  it('refuses an empty plan, because a return of nothing is not a document', () => {
    expect(planReturn([line('i1', 10)], new Map(), new Map([['i1', 0]]))).toBeNull();
    expect(planReturn([], new Map(), new Map())).toBeNull();
  });

  it('does not let two entries overdraw one line', () => {
    // The defect the credit note shipped and fixed, in its pure form: the plan
    // is built from the lines, so a line can appear at most once. This asserts
    // the shape that makes that true rather than trusting the caller.
    const items = [line('i1', 10)];
    const plan = planReturn(items, new Map(), new Map([['i1', 6]]));
    expect(plan).toEqual([{ purchase_item_id: 'i1', quantity: 6 }]);
    const ids = plan!.map((entry) => entry.purchase_item_id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe('parseReturnQuantity', () => {
  it('reads a blank field as zero, because an emptied line means "nothing here"', () => {
    // This is the defect a whole e2e caught: `parseQuantity` answers `null` for
    // blank and for "0", and on a return form zero is a legitimate instruction,
    // not an error. With the old parser one emptied field made the entire plan
    // invalid, so a multi-line purchase could only be returned in full.
    expect(parseReturnQuantity('')).toBe(0);
    expect(parseReturnQuantity('   ')).toBe(0);
  });

  it('reads "0" as zero rather than as an invalid quantity', () => {
    expect(parseReturnQuantity('0')).toBe(0);
    expect(parseReturnQuantity('00')).toBe(0);
  });

  it('reads a positive whole number as itself', () => {
    expect(parseReturnQuantity('1')).toBe(1);
    expect(parseReturnQuantity('42')).toBe(42);
  });

  it('refuses what is not a count, so the server never sees it', () => {
    // A fraction, a sign, a letter and a separator are all things the field does
    // not accept. `null` makes the caller show the error instead of sending a
    // request whose refusal is already known.
    expect(parseReturnQuantity('1.5')).toBeNull();
    expect(parseReturnQuantity('-1')).toBeNull();
    expect(parseReturnQuantity('2x')).toBeNull();
    expect(parseReturnQuantity('1,000')).toBeNull();
  });

  it('refuses a number too large to be exact, instead of rounding it', () => {
    expect(parseReturnQuantity('99999999999999999999')).toBeNull();
  });
});
