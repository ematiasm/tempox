import { describe, expect, it } from 'vitest';
import {
  bonusCountOf,
  canRecord,
  completeLines,
  draftTotal,
  everyLineComplete,
  hasInvalidLine,
  isInvalidLine,
  parseDraftLine,
  unitCountOf,
  type DraftLine
} from '#lib/draftLines.js';

/**
 * The line rule both draft screens share.
 *
 * This file exists because the rule used to live inside two components, as
 * `$derived` values no unit test could reach: `SaleDraft` dropped unparseable
 * lines and summed the rest, `PurchaseDraft` refused the whole form if any line
 * was unreadable. Two copies of one idea, and neither covered the case of a field
 * holding text that does not parse at all.
 *
 * The centre of gravity here is the difference between **empty** and **wrong**: an
 * empty field is one the cashier has not reached, and text that does not parse is
 * a mistake. Getting those confused makes the form shout at someone who has
 * simply not finished typing.
 *
 * Amounts here are written in the wire form (`10.00`) because that is what
 * `parseAmount` returns; the field's text is in the active language.
 */

function line(qtyText: string, priceText: string): DraftLine {
  return { qtyText, priceText };
}

describe('parseDraftLine', () => {
  it('reads a line whose both fields parse', () => {
    expect(parseDraftLine(line('2', '10,00'))).toEqual({ quantity: 2, unitPrice: '10.00' });
  });

  it('is null when the quantity is empty, however good the price is', () => {
    expect(parseDraftLine(line('', '10,00'))).toBeNull();
  });

  it('is null when the price is empty, however good the quantity is', () => {
    expect(parseDraftLine(line('2', ''))).toBeNull();
  });

  it('is null when the quantity is not a whole count', () => {
    expect(parseDraftLine(line('1.5', '10,00'))).toBeNull();
    expect(parseDraftLine(line('0', '10,00'))).toBeNull();
    expect(parseDraftLine(line('-1', '10,00'))).toBeNull();
    expect(parseDraftLine(line('dos', '10,00'))).toBeNull();
  });

  it('is null when the price is not an amount', () => {
    expect(parseDraftLine(line('2', 'diez'))).toBeNull();
    expect(parseDraftLine(line('2', '10,005'))).toBeNull();
  });

  it('accepts the es-AR shape a cashier actually types', () => {
    // The field is read in the active language, which defaults to es-AR.
    expect(parseDraftLine(line('2', '1.234,56'))).toEqual({ quantity: 2, unitPrice: '1234.56' });
  });
});

describe('parseDraftLine is the single answer to "is this line money yet"', () => {
  it('is null unless both fields parse', () => {
    expect(parseDraftLine(line('1', '10,00'))).not.toBeNull();
    expect(parseDraftLine(line('', '10,00'))).toBeNull();
    expect(parseDraftLine(line('1', ''))).toBeNull();
  });
});

describe('completeLines', () => {
  it('keeps the lines that count and drops the rest, in order', () => {
    const complete = completeLines([line('1', '10,00'), line('', ''), line('2', '5,00')]);
    expect(complete.map((c) => c.quantity)).toEqual([1, 2]);
    expect(complete.map((c) => c.unitPrice)).toEqual(['10.00', '5.00']);
  });

  it('is empty for a form with no lines', () => {
    expect(completeLines([])).toEqual([]);
  });

  it('is empty when no line parses, rather than failing', () => {
    expect(completeLines([line('', ''), line('x', 'y')])).toEqual([]);
  });

  it('carries the original line through, so the component keeps its identity', () => {
    const original = { ...line('1', '10,00'), id: 'l1', name: 'Harina' };
    const complete = completeLines([original]);
    expect(complete[0]).toMatchObject({
      id: 'l1',
      name: 'Harina',
      quantity: 1,
      unitPrice: '10.00'
    });
  });

  it('does not mutate the lines it was given', () => {
    const lines = [line('1', '10,00')];
    completeLines(lines);
    expect(lines).toEqual([line('1', '10,00')]);
  });
});

describe('isInvalidLine / hasInvalidLine', () => {
  it('does not call an empty field an error, because the cashier has not reached it', () => {
    expect(isInvalidLine(line('', ''))).toBe(false);
    expect(isInvalidLine(line('1', ''))).toBe(false);
    expect(isInvalidLine(line('', '10,00'))).toBe(false);
  });

  it('calls present-but-unreadable text an error', () => {
    expect(isInvalidLine(line('dos', '10,00'))).toBe(true);
    expect(isInvalidLine(line('1', 'diez'))).toBe(true);
  });

  it('ignores whitespace, so a field holding only spaces is still empty', () => {
    expect(isInvalidLine(line('   ', '10,00'))).toBe(false);
    expect(isInvalidLine(line('1', '   '))).toBe(false);
  });

  it('is false for a form with no lines at all', () => {
    expect(hasInvalidLine([])).toBe(false);
  });

  it('reports one bad line among good ones', () => {
    expect(hasInvalidLine([line('1', '10,00'), line('dos', '10,00')])).toBe(true);
  });
});

describe('draftTotal', () => {
  it('is zero for no complete lines, so an empty draft previews as zero', () => {
    expect(draftTotal([])).toBe('0.00');
  });

  it('sums one line exactly', () => {
    expect(draftTotal([{ quantity: 3, unitPrice: '10.00' }])).toBe('30.00');
  });

  it('sums several lines as whole cents, with no float error', () => {
    // 0.10 + 0.20 is the classic float trap: 0.30000000000000004 as a number.
    expect(
      draftTotal([
        { quantity: 1, unitPrice: '0.10' },
        { quantity: 1, unitPrice: '0.20' }
      ])
    ).toBe('0.30');
  });

  it('keeps a total above the float-safe range exact', () => {
    // 9007199254740993 (2^53 + 1) cannot be represented as a float.
    expect(draftTotal([{ quantity: 1, unitPrice: '90071992547409.93' }])).toBe('90071992547409.93');
  });

  it('counts a bonus line as zero without dropping it', () => {
    expect(
      draftTotal([
        { quantity: 2, unitPrice: '10.00' },
        { quantity: 1, unitPrice: '0.00' }
      ])
    ).toBe('20.00');
  });

  it('produces a well-formed Amount, with two decimals', () => {
    expect(draftTotal([{ quantity: 1, unitPrice: '5.50' }])).toBe('5.50');
    expect(draftTotal([{ quantity: 1, unitPrice: '0.05' }])).toBe('0.05');
  });
});

describe('draftTotal refuses rather than skipping a line it cannot total', () => {
  it('is null when a line total cannot be computed, instead of under-reporting', () => {
    // `parseDraftLine` cannot produce this shape today (it refuses a non-positive
    // quantity and a malformed price), so this pins the *policy* for the day the
    // two parsers start to disagree: a plausible-looking wrong total is worse than
    // a dash.
    expect(draftTotal([{ quantity: 0, unitPrice: '10.00' }])).toBeNull();
    expect(draftTotal([{ quantity: 1, unitPrice: 'nope' as never }])).toBeNull();
  });

  it('reports a figure when every line can be totalled', () => {
    expect(draftTotal([{ quantity: 1, unitPrice: '10.00' }])).toBe('10.00');
  });
});

describe('canRecord', () => {
  it('is true with one complete line and nothing in flight', () => {
    expect(canRecord([line('1', '10,00')], false)).toBe(true);
  });

  it('is false with no lines at all', () => {
    expect(canRecord([], false)).toBe(false);
  });

  it('is false when no line parses, even though lines exist', () => {
    expect(canRecord([line('', '')], false)).toBe(false);
  });

  it('is false while a submission is in flight, so it cannot be sent twice', () => {
    expect(canRecord([line('1', '10,00')], true)).toBe(false);
  });

  it('is true with a complete line beside a half-typed one', () => {
    // The sales sheet's rule: a line being typed must not block recording the rest.
    expect(canRecord([line('1', '10,00'), line('', '')], false)).toBe(true);
  });
});

describe('everyLineComplete', () => {
  it('is true when every line parses', () => {
    expect(everyLineComplete([line('1', '10,00'), line('2', '5,00')])).toBe(true);
  });

  it('is false when one line does not parse, even if others do', () => {
    // The purchase sheet's rule: a purchase is a supplier's document, so a
    // partial one must not be recordable.
    expect(everyLineComplete([line('1', '10,00'), line('', '')])).toBe(false);
  });

  it('is false for no lines, because there is nothing to record', () => {
    expect(everyLineComplete([])).toBe(false);
  });
});

describe('unitCountOf', () => {
  it('adds the whole units of the complete lines', () => {
    expect(
      unitCountOf([
        { quantity: 2, unitPrice: '1.00' },
        { quantity: 3, unitPrice: '1.00' }
      ])
    ).toBe(5);
  });

  it('is zero for no lines', () => {
    expect(unitCountOf([])).toBe(0);
  });
});

describe('bonusCountOf', () => {
  it('counts the lines priced at zero', () => {
    expect(
      bonusCountOf([
        { quantity: 1, unitPrice: '0.00' },
        { quantity: 1, unitPrice: '10.00' },
        { quantity: 5, unitPrice: '0.00' }
      ])
    ).toBe(2);
  });

  it('does not call a cheap line a bonus line', () => {
    expect(bonusCountOf([{ quantity: 1, unitPrice: '0.01' }])).toBe(0);
  });

  it('is zero for no lines', () => {
    expect(bonusCountOf([])).toBe(0);
  });
});
