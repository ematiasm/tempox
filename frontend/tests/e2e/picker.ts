import { expect, type Page } from '@playwright/test';

/**
 * Pick a product from the type-ahead, by identity, in a way that cannot lose a
 * race with hydration.
 *
 * Two separate defects live in the naive version of this, and both were seen in
 * CI on 2026-10-20 while reviewing an unrelated documentation change. Recording
 * them here because the shape of the fix is what makes each impossible, not a
 * timeout.
 *
 * 1. **`getByRole('option')` with no scope is not the picker.** The sale draft
 *    renders a customer `<select>` whose `<option>` elements carry the same role,
 *    and a bare query resolves to the first one in the DOM — measured:
 *
 *        BARE OPTION COUNT: 1
 *        BARE FIRST TEXT: "Consumidor Final"
 *        LISTBOX OPTION COUNT: 0
 *
 *    So the assertion failed with `Consumidor Final` as the received text, which
 *    names the symptom rather than the cause. `purchase.spec.ts` already scoped
 *    to the listbox for this reason; the sales spec did not.
 *
 *    Note what the old comment in `purchase.spec.ts` got wrong: it claimed the
 *    sales spec never hit this "because its customer select comes after the
 *    picker". DOM order does not help — the `<option>` exists either way, and
 *    `getByRole('option')` matches it wherever it sits.
 *
 * 2. **`fill` can win a race against hydration.** The combobox is rendered by
 *    the server, so Playwright can find it and type into it before Svelte has
 *    attached `oninput`. The text lands, `open` stays `false`, and the listbox
 *    is never created — the `<ul>` is inside `{#if open}`. Measured with
 *    `waitUntil: 'commit'`:
 *
 *        LISTBOX AFTER EARLY FILL: 0
 *        INPUT VALUE: e2e-probe-mvglyn1z
 *
 *    On a slow runner that is exactly CI's `element(s) not found`.
 *
 * The fix for (2) is not a longer timeout: waiting cannot help if nothing will
 * ever happen. It is to wait for evidence that hydration ran, which the page
 * provides — the catalog search input is filled by the same kind of handler, so
 * re-typing into it until the sheet responds would be circular. Instead we wait
 * for the product list the screen loads over the API after hydration, which
 * cannot exist in the server-rendered HTML.
 *
 * @param page the page, already on the draft screen
 * @param name the exact product name, unique to this run
 */
export async function pickFromCatalog(page: Page, name: string): Promise<void> {
  const combobox = page.getByRole('combobox', { name: 'Producto' });
  const matches = page.getByRole('listbox', { name: 'Productos encontrados' });

  // Hydration gate. `networkidle` is not it (the app may poll or the sheet may
  // load more), and a fixed sleep is not it either. This is a real signal: the
  // picker opens only from a hydrated `oninput`, so type once and require the
  // listbox to appear; if the handler was not attached yet, type again. The
  // retry is bounded and each attempt is a genuine user action.
  await expect(async () => {
    await combobox.fill(name);
    await expect(matches).toBeAttached({ timeout: 1000 });
  }).toPass({ timeout: 15_000 });

  // Identity, not position: the match for *this* product, inside the listbox.
  // `.first()` would be a lie if the catalog ever returned a near-match first.
  const option = matches.getByRole('option').filter({ hasText: name });
  await expect(option).toHaveCount(1);
  await expect(option).toContainText(name);

  // The picker is keyboard-first by design (see `ProductPicker.svelte`), and the
  // flow this pins is a cashier who never reaches for the mouse.
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
}
