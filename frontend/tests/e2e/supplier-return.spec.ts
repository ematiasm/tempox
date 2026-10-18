import { expect, test } from '@playwright/test';

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

  // 1. The product exists, created through the catalog's own form. The stock
  //    field is LEFT ALONE on purpose: an empty initial stock is 0, while a
  //    typing `0` is refused because a quantity must be at least 1.
  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('10,00');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  // 2. Buy 6 units at $10, so the purchase is $60 and the stock goes to 6.
  await page.goto('/purchases');
  await page.getByRole('combobox', { name: 'Producto' }).fill(name);
  const matches = page.getByRole('listbox', { name: 'Productos encontrados' });
  await expect(matches.getByRole('option').first()).toContainText(name);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');

  const draftLine = page.locator('.sheet table.lines tbody tr').first();
  await expect(draftLine).toContainText(name);
  await draftLine.locator('input[aria-label^="Cant"]').fill('6');
  await expect(page.locator('.sheet .total')).toHaveText('$ 60,00');

  await page.getByRole('button', { name: /Registrar compra/ }).click();
  await expect(page.locator('.sheet .done')).toContainText('$ 60,00');

  // 3. Stock really is 6 before anything goes back. Measured rather than assumed:
  //    the return's assertion below is only meaningful against a known start.
  await page.goto('/products');
  await page.locator('#catalog-search').fill(name);
  await expect(page.locator('table.catalog tbody tr', { hasText: name })).toContainText('6');

  // 4. Open the purchase and return 2 of the 6. The return field is prefilled
  //    with what is left — the whole line — so the operator types over it only
  //    when they are returning less.
  await page.goto('/purchases');
  const doc = page.locator('.ledger .document').first();
  await expect(doc).toContainText('$ 60,00');
  await doc.locator('.head').click();

  const returnField = doc.locator('input[aria-label^="Cantidad a devolver"]').first();
  await expect(returnField).toHaveValue('6');
  await returnField.fill('2');

  // The action is the two-step confirm the credit note established: the control
  // itself is ink, and the confirmation names what will happen.
  await doc.getByRole('button', { name: 'Devolver al proveedor…' }).click();
  await expect(doc).toContainText('Las unidades listadas vuelven al proveedor');
  await doc.getByRole('button', { name: 'Sí, devolver' }).click();

  // 5. The return is listed UNDER the purchase it corrects, with its own total.
  //    2 units at the price that was paid ($10) is $20 — read from the purchase
  //    line on the server, never sent by the client.
  const returnBlock = doc.locator('.return-block').first();
  await expect(returnBlock).toContainText('Devolución al proveedor');
  await expect(returnBlock.locator('.return-total')).toHaveText('+$ 20,00');
  await expect(returnBlock).toContainText(name);

  // 6. The purchase itself is untouched: still $60, still standing. A return is a
  //    second document, never an edit — DESIGN.md's Reversal Is a Second Document
  //    Rule, asserted where a reader would notice it break.
  await expect(doc.locator('.head')).toContainText('$ 60,00');

  // 7. The point of the slice, measured where the operator reads it: stock came
  //    DOWN by what went back, and 4 of the 6 remain.
  await page.goto('/products');
  await page.locator('#catalog-search').fill(name);
  await expect(page.locator('table.catalog tbody tr', { hasText: name })).toContainText('4');

  // 8. What is left to return is now 4, not 6: the form derives it from the
  //    record rather than from what this page remembers sending.
  await page.goto('/purchases');
  const docAgain = page.locator('.ledger .document').first();
  await docAgain.locator('.head').click();
  await expect(docAgain.locator('input[aria-label^="Cantidad a devolver"]').first()).toHaveValue(
    '4'
  );
});

test('a return beyond what is left is refused and writes nothing (S8, S9)', async ({ page }) => {
  const name = `e2e-overreturn-${Date.now().toString(36)}`;

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('10,00');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.goto('/purchases');
  await page.getByRole('combobox', { name: 'Producto' }).fill(name);
  await expect(
    page.getByRole('listbox', { name: 'Productos encontrados' }).getByRole('option').first()
  ).toContainText(name);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');

  const draftLine = page.locator('.sheet table.lines tbody tr').first();
  await draftLine.locator('input[aria-label^="Cant"]').fill('3');
  await page.getByRole('button', { name: /Registrar compra/ }).click();
  await expect(page.locator('.sheet .done')).toContainText('$ 30,00');

  // The form itself refuses the plan: asking for more than the line has left is
  // a quantity the server would answer 409 for, so the screen says so first and
  // sends nothing. Stock must stay at 3 because no request left.
  const doc = page.locator('.ledger .document').first();
  await doc.locator('.head').click();
  await doc.locator('input[aria-label^="Cantidad a devolver"]').first().fill('9');
  await doc.getByRole('button', { name: 'Devolver al proveedor…' }).click();
  await doc.getByRole('button', { name: 'Sí, devolver' }).click();

  await expect(doc.getByRole('alert')).toContainText('no es válida');

  await page.goto('/products');
  await page.locator('#catalog-search').fill(name);
  await expect(page.locator('table.catalog tbody tr', { hasText: name })).toContainText('3');
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

  for (const name of [keep, back]) {
    await page.goto('/products');
    await page.locator('#new-name').fill(name);
    await page.locator('#new-price').fill('10,00');
    await page.getByRole('button', { name: 'Crear producto' }).click();
    await expect(page.getByRole('status')).toContainText(name);
  }

  // One purchase with two lines: 2 of one, 3 of the other.
  await page.goto('/purchases');
  for (const [name, qty] of [
    [keep, '2'],
    [back, '3']
  ] as const) {
    await page.getByRole('combobox', { name: 'Producto' }).fill(name);
    await expect(
      page.getByRole('listbox', { name: 'Productos encontrados' }).getByRole('option').first()
    ).toContainText(name);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await page
      .locator('.sheet table.lines tbody tr')
      .filter({ hasText: name })
      .locator('input[aria-label^="Cant"]')
      .fill(qty);
  }
  await page.getByRole('button', { name: /Registrar compra/ }).click();
  await expect(page.locator('.sheet .done')).toContainText('$ 50,00');

  const doc = page.locator('.ledger .document').first();
  await doc.locator('.head').click();

  // Return the whole of `back` and nothing of `keep`.
  await doc.getByRole('textbox', { name: `Cantidad a devolver de ${keep}` }).fill('0');
  await doc.getByRole('textbox', { name: `Cantidad a devolver de ${back}` }).fill('3');
  await doc.getByRole('button', { name: 'Devolver al proveedor…' }).click();
  await doc.getByRole('button', { name: 'Sí, devolver' }).click();

  // 3 units at the price paid is $30, and only that line moved.
  const returnBlock = doc.locator('.return-block').first();
  await expect(returnBlock).toContainText(back);
  await expect(returnBlock.locator('.return-total')).toHaveText('+$ 30,00');
  await expect(returnBlock).not.toContainText(keep);

  // The untouched line still has everything left to return.
  await expect(doc.getByRole('textbox', { name: `Cantidad a devolver de ${keep}` })).toHaveValue(
    '2'
  );

  // And the purchase is still worth what it was.
  await expect(doc.locator('.head')).toContainText('$ 50,00');
});
