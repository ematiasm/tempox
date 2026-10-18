import { expect, test } from '@playwright/test';

/**
 * The sale flow, end to end, through the real UI against the real API (S14).
 *
 * This is the regression barrier the stamp relies on. It runs the whole path —
 * create a product, find it by typing, pick it with the keyboard, record the
 * sale — and then asserts the stamp landed, because the stamp is the one piece
 * of the direction a unit test cannot see.
 *
 * Every run uses a product name unique to it, so the test never depends on what
 * a previous run left behind and never asserts on someone else's data.
 */
test('a sale is recorded from the draft sheet and wears its stamp', async ({ page }) => {
  const name = `e2e-sale-${Date.now().toString(36)}`;

  // 1. The product has to exist. It is created through the catalog's own form
  //    rather than through the API, so the test covers that screen too.
  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('25,00');
  await page.locator('#new-stock').fill('10');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  // The derived stock reads back, which proves the create actually landed.
  await page.locator('#catalog-search').fill(name);
  const row = page.locator('table.catalog tbody tr', { hasText: name });
  await expect(row).toBeVisible();
  await expect(row).toContainText('10');

  // 2. Record the sale: type-ahead, keyboard pick, quantity, live total.
  await page.goto('/sales');
  await page.getByRole('combobox', { name: 'Producto' }).fill(name);
  const option = page.getByRole('option').first();
  await expect(option).toContainText(name);

  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');

  const line = page.locator('.sheet table.lines tbody tr').first();
  await expect(line).toContainText(name);
  // The price is prefilled from the list price, written in the shop's own shape
  // so the field's parser accepts it, and stays overridable (S5).
  await expect(line.locator('input[aria-label^="Precio"]')).toHaveValue('25,00');
  await expect(page.locator('.sheet .total')).toHaveText('$ 25,00');

  await line.locator('input[aria-label^="Cant"]').fill('4');
  await expect(page.locator('.sheet .total')).toHaveText('$ 100,00');

  // 3. Record it. The band's action is the page's single next action.
  await page.getByRole('button', { name: /Registrar venta/ }).click();

  // 4. The stamp, which is what this barrier exists to protect.
  const recorded = page.locator('.recorded');
  await expect(recorded).toContainText('Confirmada');
  await expect(recorded).toContainText('$ 100,00');
  await expect(page.locator('.sheet table.lines tbody tr')).toHaveCount(0);

  // 5. And it is in the day's ledger, stamped, with its lines behind it.
  const doc = page.locator('table.docs tbody tr.doc').first();
  await expect(doc).toContainText('Consumidor Final');
  await expect(doc).toContainText('$ 100,00');
  await expect(doc.locator('.stamp')).toContainText('Confirmada');

  await doc.locator('.doc-id').click();
  await expect(page.locator('.lines table')).toContainText(name);
  await expect(page.locator('.lines table')).toContainText('$ 100,00');
});

test('a sale can be returned in parts with credit notes', async ({ page }) => {
  const name = `e2e-refund-${Date.now().toString(36)}`;

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('25,00');
  await page.locator('#new-stock').fill('10');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.goto('/sales');
  await page.getByRole('combobox', { name: 'Producto' }).fill(name);
  await expect(page.getByRole('option').first()).toContainText(name);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  const line = page.locator('.sheet table.lines tbody tr').first();
  await expect(line).toContainText(name);
  await line.locator('input[aria-label^="Cant"]').fill('4');
  await expect(page.locator('.sheet .total')).toHaveText('$ 100,00');
  await page.getByRole('button', { name: /Registrar venta/ }).click();

  const doc = page.locator('table.docs tbody tr.doc').first();
  await expect(doc.locator('.stamp')).toContainText('Confirmada');
  await doc.locator('.doc-id').click();
  const document = page.locator('.lines');
  await expect(document.locator('.note-block')).toHaveCount(0);

  const quantity = page.getByRole('textbox', { name: `Devolver ${name}` });
  await quantity.fill('1');
  await page.getByRole('button', { name: 'Anular venta…' }).click();
  await expect(document.locator('.note-block')).toHaveCount(0);
  await page.getByRole('button', { name: 'Sí, anular' }).click();
  await expect(document.locator('.note-block')).toContainText('−$ 25,00');
  await expect(doc.locator('.stamp')).toContainText('Confirmada');

  await page.getByRole('textbox', { name: `Devolver ${name}` }).fill('3');
  await page.getByRole('button', { name: 'Anular venta…' }).click();
  await page.getByRole('button', { name: 'Sí, anular' }).click();
  await expect(doc.locator('.stamp')).toContainText('Anulada');
});

/**
 * A refund of one line never touches another one.
 *
 * This is the case the suite was missing, and the defect it let through was
 * measured rather than imagined: the only refund test above records a
 * **single-line** sale, so nothing ever exercised the multi-line plan. The
 * quantity field prefilled with the line's full remaining stock, and `0` was
 * refused as an invalid quantity, so the only way to refund line A was to refund
 * line B as well. Confirming returned stock and money on a line the cashier
 * never chose.
 */
test('a refund of one line leaves the other lines untouched', async ({ page }) => {
  const stamp = Date.now().toString(36);
  const a = `e2e-pickA-${stamp}`;
  const b = `e2e-pickB-${stamp}`;

  for (const [name, price] of [
    [a, '10,00'],
    [b, '20,00']
  ]) {
    await page.goto('/products');
    await page.locator('#new-name').fill(name);
    await page.locator('#new-price').fill(price);
    await page.locator('#new-stock').fill('10');
    await page.getByRole('button', { name: 'Crear producto' }).click();
    await expect(page.getByRole('status')).toContainText(name);
  }

  // One sale with two lines: 4 of A ($40) and 3 of B ($60).
  await page.goto('/sales');
  for (const [name, quantity] of [
    [a, '4'],
    [b, '3']
  ]) {
    await page.getByRole('combobox', { name: 'Producto' }).fill(name);
    await expect(page.getByRole('option').first()).toContainText(name);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    const line = page.locator('.sheet table.lines tbody tr', { hasText: name });
    await expect(line).toContainText(name);
    await line.locator('input[aria-label^="Cant"]').fill(quantity);
  }
  await expect(page.locator('.sheet .total')).toHaveText('$ 100,00');
  await page.getByRole('button', { name: /Registrar venta/ }).click();

  const doc = page.locator('table.docs tbody tr.doc').first();
  await expect(doc.locator('.stamp')).toContainText('Confirmada');
  await doc.locator('.doc-id').click();
  const document = page.locator('.lines');
  await expect(document.locator('.note-block')).toHaveCount(0);

  // The plan starts with every line that still has room, so B is included.
  const includeB = page.getByRole('checkbox', { name: `Devolver ${b}` });
  await expect(includeB).toBeChecked();

  // Unchecking B is how the cashier says "not this line". Nothing about B moves.
  await includeB.uncheck();
  await expect(page.getByRole('textbox', { name: `Devolver ${b}` })).toBeDisabled();

  await page.getByRole('textbox', { name: `Devolver ${a}` }).fill('1');
  await page.getByRole('button', { name: 'Anular venta…' }).click();
  await page.getByRole('button', { name: 'Sí, anular' }).click();

  // The note covers one unit of A only: $10, not $10 plus B's $60.
  await expect(document.locator('.note-block')).toHaveCount(1);
  await expect(document.locator('.note-block')).toContainText('−$ 10,00');
  await expect(document.locator('.note-block')).not.toContainText(b);

  // B still has all three units left, which is the whole point.
  await expect(document).toContainText('quedan 3 de 3');
});

/**
 * A refund of nothing is refused before it leaves the screen.
 *
 * Unchecking every line and confirming must not send an empty body for the
 * server to reject with a message about a quantity: the cashier gets an answer
 * they can act on, and no request is made.
 */
test('a refund with every line excluded is refused before sending', async ({ page }) => {
  const name = `e2e-none-${Date.now().toString(36)}`;

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('30,00');
  await page.locator('#new-stock').fill('5');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.goto('/sales');
  await page.getByRole('combobox', { name: 'Producto' }).fill(name);
  await expect(page.getByRole('option').first()).toContainText(name);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: /Registrar venta/ }).click();

  const doc = page.locator('table.docs tbody tr.doc').first();
  await doc.locator('.doc-id').click();
  const document = page.locator('.lines');

  await page.getByRole('checkbox', { name: `Devolver ${name}` }).uncheck();
  await page.getByRole('button', { name: 'Anular venta…' }).click();
  await page.getByRole('button', { name: 'Sí, anular' }).click();

  await expect(page.locator('[role="alert"]')).toContainText('no es válida');
  await expect(document.locator('.note-block')).toHaveCount(0);
});

test('a bonus line is recorded and moves no money', async ({ page }) => {
  const name = `e2e-gift-${Date.now().toString(36)}`;

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('10,00');
  await page.locator('#new-stock').fill('5');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.goto('/sales');
  await page.getByRole('combobox', { name: 'Producto' }).fill(name);
  await expect(page.getByRole('option').first()).toContainText(name);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');

  // The line arrives priced; zeroing it is what makes it a regalo.
  const line = page.locator('.sheet table.lines tbody tr').first();
  await line.locator('input[aria-label^="Precio"]').fill('0,00');
  await expect(page.locator('.sheet .total')).toHaveText('$ 0,00');
  // The sheet says what that line is, so a zero total is never a silent bug.
  await expect(page.locator('.sheet')).toContainText('línea de regalo');

  await page.getByRole('button', { name: /Registrar venta/ }).click();
  await expect(page.locator('.recorded')).toContainText('Confirmada');

  // The stock still left the shop: that is the whole point of recording it.
  await page.goto('/products');
  await page.locator('#catalog-search').fill(name);
  const row = page.locator('table.catalog tbody tr', { hasText: name });
  await expect(row).toBeVisible();
  await expect(row).toContainText('4');
});
