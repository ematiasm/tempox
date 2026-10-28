import { expect, test } from '@playwright/test';
import {
  money,
  purchaseDraftLine,
  purchaseSlip,
  purchaseSlipDetail,
  recordedDocumentId
} from './unique.js';
import { pickFromCatalog } from './picker.js';

/**
 * The purchase flow, end to end, through the real UI against the real API.
 *
 * The mirror of `sale.spec.ts`, with the direction reversed: recording a
 * purchase raises stock and lowers cash. What this barrier uniquely proves is
 * that the screen and the ledger agree — a unit test can check that the client
 * sent the right body, but only this can show that the document the operator
 * just typed appears in the day's ledger with its lines behind it.
 *
 * Every run uses a product name unique to it, so the test never depends on what
 * a previous run left behind and never asserts on someone else's data.
 */
test('a purchase is recorded and raises the stock it brought in', async ({ page }) => {
  const name = `e2e-purchase-${Date.now().toString(36)}`;
  const price = '10,00';
  const oneTotal = '10,00';
  const sevenTotal = '70,00';

  // 1. The product exists first, created through the catalog's own form so this
  //    test also covers that screen. The stock field is LEFT ALONE here: this
  //    spec is about the purchase, and an empty initial stock is 0 — the same
  //    state as typing `0`, which the form now accepts (`catalog.spec.ts`
  //    proves that, because it used to refuse it).
  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill(price);
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.locator('#catalog-search').fill(name);
  const row = page.locator('table.catalog tbody tr', { hasText: name });
  await expect(row).toBeVisible();
  await expect(row).toContainText('0');

  // 2. Record the purchase: type-ahead, keyboard pick, quantity, live total.
  await page.goto('/purchases');
  await pickFromCatalog(page, name);

  const line = purchaseDraftLine(page);
  await expect(line).toContainText(name);
  // The price is prefilled from the list price, written in the shop's own shape
  // so the field's parser accepts it, and stays overridable: a purchase price
  // that differs from the catalogue is the normal case.
  await expect(line.locator('input[aria-label^="Precio"]')).toHaveValue(price);
  await expect(page.locator('.sheet .total')).toHaveText(money(oneTotal));

  await line.locator('input[aria-label^="Cant"]').fill('7');
  await expect(page.locator('.sheet .total')).toHaveText(money(sevenTotal));

  // 3. The supplier defaults to the seeded one, which is what a purchase with no
  //    named supplier means. It reads in the shop's language, not as the stored
  //    English string.
  const supplier = page.locator('#purchase-supplier');
  await expect(supplier).toHaveValue('00000000-0000-0000-0000-000000000002');
  await expect(supplier.locator('option:checked')).toHaveText('Proveedor ocasional');

  // 4. Record it. The band's action is the page's single next action.
  await page.getByRole('button', { name: /Registrar compra/ }).click();

  await expect(page.locator('.sheet .done')).toContainText(money(sevenTotal));
  await expect(page.locator('[data-draft-line]')).toHaveCount(0);
  const purchaseId = await recordedDocumentId(page);

  // 5. It is in the day's ledger with its lines behind it. Selected by the id the
  //    draft reported: position is a lie in a shared ledger and a total is not
  //    unique (#52).
  const doc = purchaseSlip(page, purchaseId);
  await expect(doc).toHaveCount(1);
  await expect(doc).toContainText('Proveedor ocasional');
  await expect(doc).toContainText(money(sevenTotal));

  await doc.click();
  // The expanded lines live in a sibling row, the way the sales ledger does it:
  // a `<tr>` cannot contain the regions a document opens onto.
  const lines = page.locator(`#purchase-lines-${purchaseId}`);
  await expect(lines).toContainText(name);
  await expect(lines).toContainText(money(sevenTotal));

  // 6. The point of the slice, measured where the operator reads it: the stock
  //    went UP by what arrived. This is the assertion that would fail if the
  //    movement were ever written with the sale's sign.
  await page.goto('/products');
  await page.locator('#catalog-search').fill(name);
  await expect(page.locator('table.catalog tbody tr', { hasText: name })).toContainText('7');
});

test('the purchase screen speaks the shop language and its English side (S3, S11)', async ({
  page
}) => {
  // The stored supplier name is the English string `one-off-supplier`; the screen
  // must never show it. Both directions are asserted, because a dictionary that
  // has the key in one language and the raw value in the other is the failure
  // this catches — and it is invisible if only the default language is checked.
  await page.goto('/purchases');

  const supplier = page.locator('#purchase-supplier option:checked');
  await expect(supplier).toHaveText('Proveedor ocasional');
  await expect(supplier).not.toHaveText('one-off-supplier');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Registrar compra');
  await expect(page.getByRole('heading', { level: 2, name: 'Libro de compras' })).toBeVisible();

  // The rail's third destination is named, not borrowed from another language.
  const rail = page.getByRole('navigation', { name: 'Principal' });
  await expect(rail.getByRole('link', { name: 'Compras' })).toBeVisible();

  await page.getByRole('button', { name: 'Cambiar idioma' }).click();

  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Record purchase');
  await expect(page.getByRole('heading', { level: 2, name: 'Purchases ledger' })).toBeVisible();
  await expect(page.locator('#purchase-supplier option:checked')).toHaveText('One-off supplier');
  const railEn = page.getByRole('navigation', { name: 'Main' });
  await expect(railEn.getByRole('link', { name: 'Purchases' })).toBeVisible();
});

/**
 * The direction holds after the redraw: a purchase takes money out of the till,
 * and a supplier return puts it back.
 *
 * S3 of `odd/tasks/purchases-receipt-pad.md`. `/purchases` was rebuilt as the
 * mirror of `/sales`, and the one thing a mirror must not do is flip the sign: a
 * sale raises the till, a purchase lowers it. That is arithmetic the component
 * never owned — the server derives cash from every document — but nothing proved
 * it end to end after the markup changed, and a redraw is exactly when a screen
 * quietly starts reading the wrong figure.
 *
 * Read from `/accounts`, which is where the till is printed, and measured as a
 * delta around this test's own documents rather than as an absolute: the
 * database is shared, so an absolute figure belongs to whatever else ran.
 */
test('a purchase lowers the till and its return raises it back', async ({ page }) => {
  const name = `e2e-direction-${Date.now().toString(36)}`;

  /**
   * The till's balance, read by the account the server marks as default.
   *
   * Not by a name: the seeded till is called "Efectivo", while "Caja del negocio"
   * is the *tick* rendered inside the row — so a `hasText: 'Caja'` filter matched
   * every row that carried the tick and then read whichever came first. That
   * mistake made this test report a purchase *raising* the till by 50 when the
   * server had lowered it by exactly 50.
   */
  const till = async () => {
    await page.goto('/accounts');
    const row = page.locator('li.row.is-default');
    await expect(row).toHaveCount(1);
    const text = (await row.locator('.figure').first().textContent()) ?? '';
    // The sign is part of the figure: `-$ 50,00` is a till that owes, and
    // stripping every non-digit turned it into `+50`. That is exactly how this
    // test first reported a purchase *raising* the till by 50 when the server
    // had lowered it by 50 — a bug in the test that looked like a bug in money.
    const negative = text.trim().startsWith('-');
    const digits = text.replace(/[^\d,]/g, '').replace(',', '.');
    return (negative ? -1 : 1) * Number(digits);
  };

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('10,00');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  const before = await till();

  // 5 units at 10.00 = 50.00 out of the till.
  await page.goto('/purchases');
  await pickFromCatalog(page, name);
  await purchaseDraftLine(page).locator('input[aria-label^="Cant"]').fill('5');
  await expect(page.locator('.sheet .total')).toHaveText(money('50,00'));
  await page.getByRole('button', { name: /Registrar compra/ }).click();
  const purchaseId = await recordedDocumentId(page);

  const afterPurchase = await till();
  expect(before - afterPurchase).toBeCloseTo(50, 2);

  // Back to the day: `till()` navigates to `/accounts` to read the figure, so the
  // purchases screen has to be re-entered before its slips exist again.
  await page.goto('/purchases');

  // Return 2 of the 5: 20.00 comes back. The flow is the two-step confirm the
  // credit note established, driven from inside the opened document.
  const doc = purchaseSlip(page, purchaseId);
  await expect(doc).toHaveCount(1);
  await doc.click();
  const detail = purchaseSlipDetail(page, purchaseId);
  await expect(detail).toHaveCount(1);
  const returnField = detail.locator('input[aria-label^="Cantidad a devolver"]').first();
  await expect(returnField).toHaveValue('5');
  await returnField.fill('2');
  await detail.getByRole('button', { name: 'Devolver al proveedor…' }).click();
  await detail.getByRole('button', { name: 'Sí, devolver' }).click();

  // Wait for the written return to land before re-reading the till: the credit
  // path is asynchronous, and reading immediately is what made this half of the
  // test report a return of 0 in a full-suite run while passing in isolation.
  await expect(detail.locator('.return-block')).toHaveCount(1);
  await expect(detail.locator('.return-block').first()).toContainText('Devolución al proveedor');

  const afterReturn = await till();
  expect(afterReturn - afterPurchase).toBeCloseTo(20, 2);
});

/**
 * A purchase cannot be recorded twice by a caller that is not the button.
 *
 * The purchase draft's `record()` checked the supplier and the lines, then set
 * `submitting` — but it never consulted it on entry, so the disable on the button
 * was the only thing stopping a second call. The button is one caller; the form's
 * submit event and the page's F2 handler are others, and a second event queued
 * before the first render lands reaches the handler regardless.
 *
 * `Catalog.svelte` names this exact symmetry in its own comment ("`SaleDraft`
 * opens with `if (!canRecord) return`, and `canRecord` takes `submitting` for
 * exactly this reason; this one had lost the symmetry"). `SaleDraft` has it.
 * `PurchaseDraft` had lost it too, and nothing covered it.
 *
 * The stake is higher than a duplicate row: a purchase raises stock and lowers
 * cash, so recording it twice moves the till twice. Measured before the fix:
 * two dispatched submits produced two purchases and the till dropped twice.
 */
test('a purchase submitted twice while in flight is recorded once', async ({ page }) => {
  const name = `e2e-dup-purchase-${Date.now().toString(36)}`;

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('10,00');
  await page.locator('#new-stock').fill('0');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  // Count the writes, and hold the first response so the second submit lands
  // while it is still in flight — the window the guard exists for.
  let posts = 0;
  await page.route('**/api/purchases', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    posts += 1;
    await new Promise((resolve) => setTimeout(resolve, 600));
    await route.continue();
  });

  await page.goto('/purchases');
  await pickFromCatalog(page, name);
  await purchaseDraftLine(page).locator('input[aria-label^="Cant"]').fill('4');
  await expect(page.locator('.sheet .total')).toHaveText(money('40,00'));

  // Dispatch the recording action twice, synchronously, the way a held key or a
  // queued event does. Going through the button would prove only that a disabled
  // button is disabled. Ctrl+Enter is the shortcut the draft really listens for:
  // the F2 the button prints is not wired on this route, which is recorded
  // separately in the feature document rather than fixed here.
  const submitTwice = () =>
    page.evaluate(() => {
      for (let i = 0; i < 2; i++) {
        window.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, cancelable: true })
        );
      }
    });

  await submitTwice();

  await expect(page.locator('.sheet .done')).toContainText(money('40,00'));
  expect(posts, 'a purchase must be written once, not twice').toBe(1);
});
