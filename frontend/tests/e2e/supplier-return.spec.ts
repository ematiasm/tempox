import { expect, test } from '@playwright/test';
import { money, recordedDocumentId } from './unique.js';
import { pickFromCatalog } from './picker.js';

/**
 * Returning goods to their supplier, end to end, through the real UI.
 *
 * What this barrier uniquely proves is the thing no unit test can: that the
 * return a cashier records actually moves stock DOWN and cash UP, and that the
 * purchase keeps every figure it was recorded with while the return sits under
 * it. A unit test can check the body the client sent; only this can show that
 * the document the operator just typed appears where they will look for it.
 *
 * It is the mirror of `purchase.spec.ts`: there stock goes up and money out,
 * here stock comes down and money back.
 *
 * Every run uses a product name unique to it, so the test never depends on what
 * a previous run left behind and never asserts on someone else's data.
 */
test('a return takes stock out and brings money back, and the purchase stands', async ({
  page
}) => {
  const name = `e2e-return-${Date.now().toString(36)}`;
  const price = '10,00';
  const purchaseTotal = '60,00';
  const returnTotal = '20,00';

  // 1. The product exists, created through the catalog's own form. The stock
  //    field is LEFT ALONE on purpose: an empty initial stock is 0, while a
  //    typing `0` is refused because a quantity must be at least 1.
  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill(price);
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  // 2. Buy 6 units, so the stock goes to 6 and the purchase has its own total.
  await page.goto('/purchases');
  await pickFromCatalog(page, name);

  const draftLine = page.locator('.sheet table.lines tbody tr').first();
  await expect(draftLine).toContainText(name);
  await draftLine.locator('input[aria-label^="Cant"]').fill('6');
  await expect(page.locator('.sheet .total')).toHaveText(money(purchaseTotal));

  await page.getByRole('button', { name: /Registrar compra/ }).click();
  await expect(page.locator('.sheet .done')).toContainText(money(purchaseTotal));
  // Read the id now: the banner belongs to this screen and is gone after navigating.
  const purchaseId = await recordedDocumentId(page);

  // 3. Stock really is 6 before anything goes back. Measured rather than assumed:
  //    the return's assertion below is only meaningful against a known start.
  await page.goto('/products');
  await page.locator('#catalog-search').fill(name);
  await expect(page.locator('table.catalog tbody tr', { hasText: name })).toContainText('6');

  // 4. Open the purchase and return 2 of the 6. The return field is prefilled
  //    with what is left — the whole line — so the operator types over it only
  //    when they are returning less.
  await page.goto('/purchases');
  // This purchase, by the id the draft reported: position is a lie in a shared
  // ledger and a total is not unique (#52).
  const doc = page.locator(`.ledger tr.doc[aria-controls="purchase-lines-${purchaseId}"]`);
  await expect(doc).toHaveCount(1);
  await expect(doc).toContainText(money(purchaseTotal));
  await doc.click();

  const region = page.locator(`#purchase-lines-${purchaseId}`);
  const returnField = region.locator('input[aria-label^="Cantidad a devolver"]').first();
  await expect(returnField).toHaveValue('6');
  await returnField.fill('2');

  // The action is the two-step confirm the credit note established: the control
  // itself is ink, and the confirmation names what will happen.
  await region.getByRole('button', { name: 'Devolver al proveedor…' }).click();
  await expect(region).toContainText('Las unidades listadas vuelven al proveedor');
  await region.getByRole('button', { name: 'Sí, devolver' }).click();

  // 5. The return is listed UNDER the purchase it corrects, with its own total.
  //    2 units at the price that was paid ($10) is $20 — read from the purchase
  //    line on the server, never sent by the client.
  const returnBlock = region.locator('.return-block').first();
  await expect(returnBlock).toContainText('Devolución al proveedor');
  await expect(returnBlock.locator('.return-total')).toHaveText(`+${money(returnTotal)}`);
  await expect(returnBlock).toContainText(name);

  // 6. The purchase itself is untouched: still $60, still standing. A return is a
  //    second document, never an edit — DESIGN.md's Reversal Is a Second Document
  //    Rule, asserted where a reader would notice it break.
  await expect(doc).toContainText(money(purchaseTotal));

  // 7. The point of the slice, measured where the operator reads it: stock came
  //    DOWN by what went back, and 4 of the 6 remain.
  await page.goto('/products');
  await page.locator('#catalog-search').fill(name);
  await expect(page.locator('table.catalog tbody tr', { hasText: name })).toContainText('4');

  // 8. What is left to return is now 4, not 6: the form derives it from the
  //    record rather than from what this page remembers sending.
  await page.goto('/purchases');
  const docAgain = page.locator(`.ledger tr.doc[aria-controls="purchase-lines-${purchaseId}"]`);
  await expect(docAgain).toHaveCount(1);
  await docAgain.click();
  const regionAgain = page.locator(`#purchase-lines-${purchaseId}`);
  await expect(regionAgain.locator('input[aria-label^="Cantidad a devolver"]').first()).toHaveValue(
    '4'
  );
});

test('more than a line has left is refused on the screen, and no request leaves (S6)', async ({
  page
}) => {
  // What this proves, precisely: the form refuses a plan the server would answer
  // 409 to, and sends NOTHING. It does not prove that a rejected return writes
  // nothing server-side (S9) -- no request reaches the server here, so there is
  // nothing for the server to reject. S9 is pinned where it belongs, by the
  // backend's own integration tests.
  const name = `e2e-overreturn-${Date.now().toString(36)}`;
  const price = '10,00';
  const purchaseTotal = '30,00';

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill(price);
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.goto('/purchases');
  await pickFromCatalog(page, name);

  const draftLine = page.locator('.sheet table.lines tbody tr').first();
  await draftLine.locator('input[aria-label^="Cant"]').fill('3');
  await page.getByRole('button', { name: /Registrar compra/ }).click();
  await expect(page.locator('.sheet .done')).toContainText(money(purchaseTotal));
  const purchaseId = await recordedDocumentId(page);

  // The form itself refuses the plan: asking for more than the line has left is
  // a quantity the server would answer 409 for, so the screen says so first and
  // sends nothing. Stock must stay at 3 because no request left.
  const doc = page.locator(`.ledger tr.doc[aria-controls="purchase-lines-${purchaseId}"]`);
  await expect(doc).toHaveCount(1);
  await doc.click();
  const region = page.locator(`#purchase-lines-${purchaseId}`);
  await region.locator('input[aria-label^="Cantidad a devolver"]').first().fill('9');
  await region.getByRole('button', { name: 'Devolver al proveedor…' }).click();
  await region.getByRole('button', { name: 'Sí, devolver' }).click();

  await expect(region.getByRole('alert')).toContainText('no es válida');

  // The stock cell, not the row. A row-level `toContainText('3')` also matches
  // the product NAME, which is generated from a base36 timestamp and therefore
  // contains a digit like '3' some of the time -- measured at 6.3% of runs. Such
  // an assertion passes because the name happens to hold the digit, not because
  // the stock is right, and it would keep passing if the stock were 0.
  await page.goto('/products');
  await page.locator('#catalog-search').fill(name);
  const catalogRow = page.locator('table.catalog tbody tr', { hasText: name });
  await expect(catalogRow).toBeVisible();
  await expect(catalogRow.locator('.stock')).toHaveText('3');
});

test('a return of one line leaves the other line alone, which is the normal case (S6)', async ({
  page
}) => {
  // The defect this pins: the return field went through `parseQuantity`, which
  // answers `null` for `0` and for a blank field because zero units is not a
  // movement. On this form zero is how the operator says "nothing from this
  // line", so one emptied field turned the whole plan to `null` and NO request
  // left. The only return this screen accepted was "every line, all of it",
  // which is the rarest case there is.
  const stamp = Date.now().toString(36);
  const keep = `e2e-keep-${stamp}`;
  const back = `e2e-back-${stamp}`;
  const price = '10,00';
  const purchaseTotal = '50,00';
  const backTotal = '30,00';

  for (const name of [keep, back]) {
    await page.goto('/products');
    await page.locator('#new-name').fill(name);
    await page.locator('#new-price').fill(price);
    await page.getByRole('button', { name: 'Crear producto' }).click();
    await expect(page.getByRole('status')).toContainText(name);
  }

  // One purchase with two lines: 2 of one, 3 of the other.
  await page.goto('/purchases');
  for (const [name, qty] of [
    [keep, '2'],
    [back, '3']
  ] as const) {
    await pickFromCatalog(page, name);
    await page
      .locator('.sheet table.lines tbody tr')
      .filter({ hasText: name })
      .locator('input[aria-label^="Cant"]')
      .fill(qty);
  }
  await page.getByRole('button', { name: /Registrar compra/ }).click();
  await expect(page.locator('.sheet .done')).toContainText(money(purchaseTotal));
  const purchaseId = await recordedDocumentId(page);

  const doc = page.locator(`.ledger tr.doc[aria-controls="purchase-lines-${purchaseId}"]`);
  await expect(doc).toHaveCount(1);
  await doc.click();
  const region = page.locator(`#purchase-lines-${purchaseId}`);

  // Return the whole of `back` and nothing of `keep`.
  await region.getByRole('textbox', { name: `Cantidad a devolver de ${keep}` }).fill('0');
  await region.getByRole('textbox', { name: `Cantidad a devolver de ${back}` }).fill('3');
  await region.getByRole('button', { name: 'Devolver al proveedor…' }).click();
  await region.getByRole('button', { name: 'Sí, devolver' }).click();

  // 3 units at the price paid, and only that line moved.
  const returnBlock = region.locator('.return-block').first();
  await expect(returnBlock).toContainText(back);
  await expect(returnBlock.locator('.return-total')).toHaveText(`+${money(backTotal)}`);
  await expect(returnBlock).not.toContainText(keep);

  // The untouched line still has everything left to return.
  await expect(region.getByRole('textbox', { name: `Cantidad a devolver de ${keep}` })).toHaveValue(
    '2'
  );

  // And the purchase is still worth what it was.
  await expect(doc).toContainText(money(purchaseTotal));
});
