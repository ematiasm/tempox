/**
 * What the catalog create form holds, and what of it is ready for the API.
 *
 * This belongs outside `Catalog.svelte` for the same reason `draftLines.ts`
 * does: the form's rule must have one answer that both the component and a unit
 * test can reach. Before this module, nested `$derived` ternaries chose one
 * warning and one hint, so a second invalid field could be marked red without
 * any explanation.
 *
 * Every function is pure and takes plain strings, so the component owns the
 * reactive state and only asks this module what it means. Nothing here knows
 * that Svelte exists.
 */
import type { Amount } from './locale.js';
import { parseAmount, parseInitialStock } from './money.js';

/** The exact text held by the create form's fields. */
export type ProductFormText = {
  readonly nameText: string;
  readonly priceText: string;
  readonly stockText: string;
};

/** One field-level reason the form cannot yet be submitted. */
export type ProductFormIssue = {
  readonly kind: 'missing' | 'invalid';
  readonly field: 'name' | 'price' | 'stock';
};

/** Values ready for the product API, after every field has passed its rule. */
type ParsedProductForm = {
  readonly name: string;
  readonly listPrice: Amount;
  readonly initialStock: number;
};

/**
 * The create form's complete parse result, in the order an operator reads it.
 *
 * Empty name and price fields are missing rather than invalid; an empty stock
 * field is the ordinary initial state of an empty shelf, so it means zero.
 */
export function parseProductForm(text: ProductFormText): {
  issues: ProductFormIssue[];
  parsed: ParsedProductForm | null;
} {
  const issues: ProductFormIssue[] = [];
  const name = text.nameText.trim();
  const priceText = text.priceText.trim();
  const stockText = text.stockText.trim();
  const listPrice = parseAmount(text.priceText);
  const initialStock = parseInitialStock(text.stockText);

  if (name === '') issues.push({ kind: 'missing', field: 'name' });
  if (priceText === '') issues.push({ kind: 'missing', field: 'price' });
  else if (listPrice === null) issues.push({ kind: 'invalid', field: 'price' });
  if (stockText !== '' && initialStock === null) issues.push({ kind: 'invalid', field: 'stock' });

  if (issues.length > 0 || listPrice === null || initialStock === null) {
    return { issues, parsed: null };
  }

  return { issues, parsed: { name, listPrice, initialStock } };
}
