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
 * ever happen, because `open` is already `false` and no event is coming. The fix
 * is to make the input event happen again — after hydration — and to require the
 * listbox to attach as proof that the handler is live. The retry is bounded
 * (15s) and each attempt is a real user action, so it cannot spin forever on a
 * genuinely broken screen: a picker that never opens still fails, just later.
 *
 * What this deliberately does NOT do is wait for a fixed sleep or for
 * `networkidle`; neither is evidence that Svelte attached `oninput`, and both
 * would pass on a fast machine while staying green for the wrong reason.
 *
 * @param page the page, already on the draft screen
 * @param name the exact product name, unique to this run
 */
export async function pickFromCatalog(page: Page, name: string): Promise<void> {
  const combobox = page.getByRole('combobox', { name: 'Producto' });
  const matches = page.getByRole('listbox', { name: 'Productos encontrados' });

  // Type until the listbox attaches. The condition is the listbox existing at
  // all — a fact only a hydrated `oninput` can produce — so a retry means the
  // handler was not attached yet, not that the assertion was too strict.
  //
  // This helper owns the typing: callers must not `fill` the combobox first, or
  // the pre-hydration value lands and the gate below types over it.
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
