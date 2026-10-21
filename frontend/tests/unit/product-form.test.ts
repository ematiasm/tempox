import { describe, expect, it } from 'vitest';
import { type ProductFormText, parseProductForm } from '#lib/productForm.js';

/**
 * The catalog's create rule, lifted out of the component.
 *
 * It lives here for the reason `draftLines.ts` does: inside `Catalog.svelte` the
 * rule was two nested `$derived` ternaries, and neither a unit test nor a screen
 * reader could reach it. The review found the consequence — one `<p class="warn">`
 * carrying a single sentence, chosen by a ternary, so the second invalid field in
 * a two-error form was drawn red with nothing explaining it.
 *
 * The centre of gravity is the same one `draftLines.ts` states: **empty is not
 * wrong**. A blank name is a field the operator has not reached yet; text that
 * does not parse is a mistake. A blank stock is neither — it is zero.
 */

function form(name: string, price: string, stock: string): ProductFormText {
  return { nameText: name, priceText: price, stockText: stock };
}

describe('parseProductForm', () => {
  it('reads a complete form, with a blank stock meaning zero', () => {
    const result = parseProductForm(form('Harina 000', '2500', ''));
    expect(result.issues).toEqual([]);
    expect(result.parsed).toEqual({ name: 'Harina 000', listPrice: '2500.00', initialStock: 0 });
  });

  it('reads a typed zero stock as zero, which is the defect this form shipped with', () => {
    const result = parseProductForm(form('Harina 000', '2500', '0'));
    expect(result.issues).toEqual([]);
    expect(result.parsed?.initialStock).toBe(0);
  });

  it('trims the name the operator typed, once, at this boundary', () => {
    expect(parseProductForm(form('  Yerba  ', '10', '1')).parsed?.name).toBe('Yerba');
  });

  it('calls a blank name and a blank price missing, not invalid', () => {
    // The distinction the form must not lose: a field nobody has reached yet is
    // not a mistake, and shouting "not a valid number" at it is a lie.
    const result = parseProductForm(form('', '', ''));
    expect(result.issues).toEqual([
      { kind: 'missing', field: 'name' },
      { kind: 'missing', field: 'price' }
    ]);
    expect(result.parsed).toBeNull();
  });

  it('calls text that does not parse invalid, and says which field', () => {
    const price = parseProductForm(form('Harina', '2500.00', '1'));
    expect(price.issues).toEqual([{ kind: 'invalid', field: 'price' }]);

    const stock = parseProductForm(form('Harina', '2500', '-1'));
    expect(stock.issues).toEqual([{ kind: 'invalid', field: 'stock' }]);
  });

  it('reports every issue, not only the first one', () => {
    // R2-001. The component rendered one sentence because a ternary can only pick
    // one branch, so the stock field was drawn wrong with no explanation. The
    // rule has to answer for all of them; how they are shown is the view's
    // business, but the view cannot show what the rule never reported.
    const result = parseProductForm(form('', '2500.00', '-1'));
    expect(result.issues).toHaveLength(3);
    expect(result.issues.map((i) => i.field)).toEqual(['name', 'price', 'stock']);
  });

  it('orders its issues as the form reads, so a screen reader follows the layout', () => {
    const result = parseProductForm(form('', 'abc', 'xyz'));
    expect(result.issues.map((i) => `${i.field}:${i.kind}`)).toEqual([
      'name:missing',
      'price:invalid',
      'stock:invalid'
    ]);
  });

  it('never reports an issue for a valid stock, blank or typed', () => {
    for (const stock of ['', '0', ' 0 ', '12']) {
      expect(parseProductForm(form('Harina', '2500', stock)).issues).toEqual([]);
    }
  });
});
