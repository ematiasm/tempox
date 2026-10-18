import { describe, expect, it } from 'vitest';
import {
  isFullyRefunded,
  parseRefundQuantity,
  planRefund,
  refundableOf,
  refundedByLine
} from '#lib/salesRefund.js';
import type { CreditNote, SaleItem } from '#lib/api.js';

/**
 * What a sale can still give back, and which refund a form would send (S1-S4).
 *
 * These functions used to live inside `Ledger.svelte`, where no unit test could
 * reach them, and the cost was measured rather than imagined: a multi-line sale
 * could not be refunded one line at a time. `parseQuantity` answers `null` for
 * `0`, so the only way to say "nothing from this line" was refused as an invalid
 * quantity; the field prefilled with the full remaining amount, so leaving it
 * alone submitted it. The result was a refund the cashier never asked for.
 *
 * The purchase side already learned this: `supplierReturns.ts` exists for the
 * same reason and its header names the sales ledger as the place the arithmetic
 * was unreachable. This file is that extraction, with the line-selection rule the
 * screen needs to avoid proposing a plan it must not send.
 */

function line(id: string, quantity: number, unitPrice = '10.00'): SaleItem {
  return {
    id,
    sale_id: 's1',
    product_id: `prod-${id}`,
    product_name: `Product ${id}`,
    quantity,
    unit_price: unitPrice,
    line_total: `${(quantity * Number(unitPrice)).toFixed(2)}`,
    unit: 'unit'
  };
}

function creditNote(id: string, items: { sale_item_id: string; quantity: number }[]): CreditNote {
  return {
    id,
    sale_id: 's1',
    total: '0.00',
    created_at: '2026-10-18T12:00:00Z',
    items: items.map((item) => ({
      id: `${id}-${item.sale_item_id}`,
      credit_note_id: id,
      sale_item_id: item.sale_item_id,
      product_id: `prod-${item.sale_item_id}`,
      product_name: `Product ${item.sale_item_id}`,
      quantity: item.quantity,
      unit_price: '10.00',
      line_total: '0.00',
      unit: 'unit' as const
    }))
  };
}

describe('refundedByLine', () => {
  it('sums several credit notes for the same sale line', () => {
    // Returning in parts is the normal case: the cashier takes back one unit now
    // and the rest next week. What went back is the sum, never the last note.
    const refunded = refundedByLine([
      creditNote('n1', [{ sale_item_id: 'i1', quantity: 1 }]),
      creditNote('n2', [{ sale_item_id: 'i1', quantity: 2 }])
    ]);
    expect(refunded.get('i1')).toBe(3);
  });

  it('keys each line separately, so returning one never moves another', () => {
    const refunded = refundedByLine([
      creditNote('n1', [
        { sale_item_id: 'i1', quantity: 4 },
        { sale_item_id: 'i2', quantity: 1 }
      ])
    ]);
    expect(refunded.get('i1')).toBe(4);
    expect(refunded.get('i2')).toBe(1);
  });

  it('is empty for a sale with no credit notes', () => {
    expect(refundedByLine([]).size).toBe(0);
  });
});

describe('refundableOf', () => {
  it('is what was sold minus what came back', () => {
    const refunded = refundedByLine([creditNote('n1', [{ sale_item_id: 'i1', quantity: 4 }])]);
    expect(refundableOf('i1', 10, refunded)).toBe(6);
  });

  it('never goes negative, even if the record holds more than was sold', () => {
    // The server prevents over-refunding, so this is defensive. A negative "left"
    // would render as a quantity the cashier could then send.
    const refunded = refundedByLine([creditNote('n1', [{ sale_item_id: 'i1', quantity: 11 }])]);
    expect(refundableOf('i1', 10, refunded)).toBe(0);
  });

  it('treats a line with no credit notes as fully refundable', () => {
    expect(refundableOf('i1', 10, new Map())).toBe(10);
  });
});

describe('isFullyRefunded', () => {
  it('is true only when every line has come back in full', () => {
    const items = [line('i1', 10), line('i2', 4)];
    const notes = [
      creditNote('n1', [
        { sale_item_id: 'i1', quantity: 10 },
        { sale_item_id: 'i2', quantity: 4 }
      ])
    ];
    expect(isFullyRefunded(items, notes)).toBe(true);
  });

  it('is false when one line has a single unit left', () => {
    // A partial refund is not an annulment: the document still stands, wearing
    // its confirmed stamp, with a note underneath. The screen must keep offering
    // the action.
    const items = [line('i1', 10), line('i2', 4)];
    const notes = [
      creditNote('n1', [
        { sale_item_id: 'i1', quantity: 10 },
        { sale_item_id: 'i2', quantity: 3 }
      ])
    ];
    expect(isFullyRefunded(items, notes)).toBe(false);
  });

  it('is false for a sale with no lines, because there is nothing to annul', () => {
    expect(isFullyRefunded([], [])).toBe(false);
  });
});

describe('parseRefundQuantity', () => {
  it('reads a blank field and a zero as "nothing from this line"', () => {
    // This is the whole defect. On a refund form zero is meaningful: it is how
    // the cashier says "not this one" while returning another. `parseQuantity`
    // answers `null` for it, which made the entire plan invalid and no request
    // leave -- so a multi-line sale could not be partially refunded at all.
    expect(parseRefundQuantity('')).toBe(0);
    expect(parseRefundQuantity('0')).toBe(0);
    expect(parseRefundQuantity('  0  ')).toBe(0);
  });

  it('reads a positive whole number as itself', () => {
    expect(parseRefundQuantity('1')).toBe(1);
    expect(parseRefundQuantity('7')).toBe(7);
    expect(parseRefundQuantity(' 12 ')).toBe(12);
  });

  it('refuses anything that is not a whole count', () => {
    // A fraction, a negative, a stray letter: none of them is a quantity, and
    // the caller shows the error rather than sending it.
    expect(parseRefundQuantity('1.5')).toBeNull();
    expect(parseRefundQuantity('-1')).toBeNull();
    expect(parseRefundQuantity('abc')).toBeNull();
    expect(parseRefundQuantity('1,5')).toBeNull();
  });
});

describe('planRefund', () => {
  it('excludes an unchecked line instead of refunding it at full', () => {
    // S1: the defect. A two-line sale where only the first line is selected must
    // send one entry, not two. Before the fix the unselected line kept its
    // prefilled full quantity and went back with the rest.
    const items = [line('i1', 4), line('i2', 3)];
    const selected = new Map<string, boolean>([
      ['i1', true],
      ['i2', false]
    ]);
    const requested = new Map<string, number | null>([
      ['i1', 1],
      ['i2', 3]
    ]);
    expect(planRefund(items, new Map(), selected, requested)).toEqual([
      { sale_item_id: 'i1', quantity: 1 }
    ]);
  });

  it('plans every selected line that still has something left', () => {
    const items = [line('i1', 10), line('i2', 4)];
    const selected = new Map<string, boolean>([
      ['i1', true],
      ['i2', true]
    ]);
    const requested = new Map<string, number | null>([
      ['i1', 6],
      ['i2', 4]
    ]);
    expect(planRefund(items, new Map(), selected, requested)).toEqual([
      { sale_item_id: 'i1', quantity: 6 },
      { sale_item_id: 'i2', quantity: 4 }
    ]);
  });

  it('skips a line that is already fully refunded, selected or not', () => {
    // The form prefills every line; a fully refunded one has nothing to offer and
    // is not an error to report.
    const items = [line('i1', 10), line('i2', 4)];
    const refunded = refundedByLine([creditNote('n1', [{ sale_item_id: 'i2', quantity: 4 }])]);
    const selected = new Map<string, boolean>([
      ['i1', true],
      ['i2', true]
    ]);
    const requested = new Map<string, number | null>([
      ['i1', 5],
      ['i2', 0]
    ]);
    expect(planRefund(items, refunded, selected, requested)).toEqual([
      { sale_item_id: 'i1', quantity: 5 }
    ]);
  });

  it('skips a selected line whose quantity reads as zero', () => {
    // S2: a selected line left at zero contributes nothing. Being explicit in the
    // field must mean the same as unchecking it.
    const items = [line('i1', 4), line('i2', 3)];
    const selected = new Map<string, boolean>([
      ['i1', true],
      ['i2', true]
    ]);
    const requested = new Map<string, number | null>([
      ['i1', 2],
      ['i2', 0]
    ]);
    expect(planRefund(items, refundedByLine([]), selected, requested)).toEqual([
      { sale_item_id: 'i1', quantity: 2 }
    ]);
  });

  it('refuses a plan asking for more than a line has left', () => {
    // The server answers 409 here. Refusing on this side is not a second
    // authority -- it is not sending a request whose refusal is already known.
    const items = [line('i1', 10)];
    const refunded = refundedByLine([creditNote('n1', [{ sale_item_id: 'i1', quantity: 4 }])]);
    const selected = new Map([['i1', true]]);
    expect(planRefund(items, refunded, selected, new Map([['i1', 7]]))).toBeNull();
  });

  it('accepts a plan asking for exactly what is left, the boundary the server allows', () => {
    const items = [line('i1', 10)];
    const refunded = refundedByLine([creditNote('n1', [{ sale_item_id: 'i1', quantity: 4 }])]);
    const selected = new Map([['i1', true]]);
    expect(planRefund(items, refunded, selected, new Map([['i1', 6]]))).toEqual([
      { sale_item_id: 'i1', quantity: 6 }
    ]);
  });

  it('refuses a selected line whose quantity is not a whole count', () => {
    const items = [line('i1', 10)];
    const selected = new Map([['i1', true]]);
    expect(planRefund(items, new Map(), selected, new Map([['i1', null]]))).toBeNull();
    expect(planRefund(items, new Map(), selected, new Map([['i1', 1.5]]))).toBeNull();
  });

  it('refuses an empty plan, because a refund of nothing is not a document', () => {
    // S3: confirming with no line selected must not send an empty body that the
    // server rejects as a 400 with an unexplained message.
    const items = [line('i1', 10)];
    const noneSelected = new Map([['i1', false]]);
    expect(planRefund(items, new Map(), noneSelected, new Map([['i1', 5]]))).toBeNull();
    expect(planRefund([], new Map(), new Map(), new Map())).toBeNull();
  });

  it('treats a line with no selection entry as unchecked rather than guessing', () => {
    // The conservative direction matters: an absent entry can only ever
    // under-refund. Guessing "selected" would refund something nobody chose.
    const items = [line('i1', 4)];
    expect(planRefund(items, new Map(), new Map(), new Map([['i1', 4]]))).toBeNull();
  });
});
