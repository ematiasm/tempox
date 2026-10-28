import { expect, test } from '@playwright/test';
import { daySlip, lineTotal, money, recordedDocumentId, saleDraftLine } from './unique.js';
import { pickFromCatalog } from './picker.js';

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
  const price = '25,00';
  const total = lineTotal(price, 4);

  // 1. The product has to exist. It is created through the catalog's own form
  //    rather than through the API, so the test covers that screen too.
  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill(price);
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
  await pickFromCatalog(page, name);

  const line = saleDraftLine(page);
  await expect(line).toContainText(name);
  // The price is prefilled from the list price, written in the shop's own shape
  // so the field's parser accepts it, and stays overridable (S5).
  await expect(line.locator('input[aria-label^="Precio"]')).toHaveValue(price);
  await expect(page.locator('.sheet .total')).toHaveText(money(price));

  await line.locator('input[aria-label^="Cant"]').fill('4');
  await expect(page.locator('.sheet .total')).toHaveText(money(total));

  // 3. Record it. The band's action is the page's single next action.
  await page.getByRole('button', { name: /Registrar venta/ }).click();

  // 4. The stamp, which is what this barrier exists to protect.
  const recorded = page.locator('.recorded');
  await expect(recorded).toContainText('Confirmada');
  await expect(recorded).toContainText(money(total));
  await expect(page.locator('[data-draft-line]')).toHaveCount(0);

  // 5. And it is in the day's ledger, stamped, with its lines behind it. Selected
  //    by the id the draft reported, not by position and not by total: every spec
  //    records into this ledger, and a total is not unique (#52).
  const saleId = await recordedDocumentId(page);
  // The row's toggle carries the full document id in `aria-controls`, which is the
  // only place the full id appears in the ledger.
  const doc = daySlip(page, saleId);
  await expect(doc).toHaveCount(1);
  await expect(doc).toContainText('Consumidor Final');
  await expect(doc).toContainText(money(total));
  await expect(doc.locator('.state')).toContainText('Confirmada');

  await doc.click();
  await expect(page.locator('.lines table')).toContainText(name);
  await expect(page.locator('.lines table')).toContainText(money(total));
});

test('a sale can be returned in parts with credit notes', async ({ page }) => {
  const name = `e2e-refund-${Date.now().toString(36)}`;
  const price = '25,00';
  const total = lineTotal(price, 4);

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill(price);
  await page.locator('#new-stock').fill('10');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.goto('/sales');
  await pickFromCatalog(page, name);
  const line = saleDraftLine(page);
  await expect(line).toContainText(name);
  await line.locator('input[aria-label^="Cant"]').fill('4');
  await expect(page.locator('.sheet .total')).toHaveText(money(total));
  await page.getByRole('button', { name: /Registrar venta/ }).click();

  // This sale, by the id the draft reported (#52).
  const saleId = await recordedDocumentId(page);
  const doc = daySlip(page, saleId);
  await expect(doc).toHaveCount(1);
  await expect(doc.locator('.state')).toContainText('Confirmada');
  await doc.click();
  const document = page.locator('.lines');
  await expect(document.locator('.note-block')).toHaveCount(0);

  const quantity = page.getByRole('textbox', { name: `Devolver ${name}` });
  await quantity.fill('1');
  await page.getByRole('button', { name: 'Anular venta…' }).click();
  await expect(document.locator('.note-block')).toHaveCount(0);
  await page.getByRole('button', { name: 'Sí, anular' }).click();
  await expect(document.locator('.note-block')).toContainText(`−${money(price)}`);
  await expect(doc.locator('.state')).toContainText('Confirmada');

  await page.getByRole('textbox', { name: `Devolver ${name}` }).fill('3');
  await page.getByRole('button', { name: 'Anular venta…' }).click();
  await page.getByRole('button', { name: 'Sí, anular' }).click();
  await expect(doc.locator('.state')).toContainText('Devuelta');
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

  const priceA = '4,37';
  // 4 x 4.37 + 3 x 20.00
  const saleTotal = '77,48';

  for (const [name, price] of [
    [a, priceA],
    [b, '20,00']
  ]) {
    await page.goto('/products');
    await page.locator('#new-name').fill(name);
    await page.locator('#new-price').fill(price);
    await page.locator('#new-stock').fill('10');
    await page.getByRole('button', { name: 'Crear producto' }).click();
    await expect(page.getByRole('status')).toContainText(name);
  }

  // One sale with two lines: 4 of A and 3 of B at $20.00.
  await page.goto('/sales');
  for (const [name, quantity] of [
    [a, '4'],
    [b, '3']
  ]) {
    await pickFromCatalog(page, name);
    const line = saleDraftLine(page, name);
    await expect(line).toContainText(name);
    await line.locator('input[aria-label^="Cant"]').fill(quantity);
  }
  await expect(page.locator('.sheet .total')).toHaveText(`$ ${saleTotal}`);
  await page.getByRole('button', { name: /Registrar venta/ }).click();

  // Select this sale by the id the draft reported. Position is a lie in a shared
  // ledger and a total is not unique (#52).
  const saleId = await recordedDocumentId(page);
  const doc = daySlip(page, saleId);
  await expect(doc).toHaveCount(1);
  await expect(doc.locator('.state')).toContainText('Confirmada');
  await doc.click();
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

  // One unit of A comes back, and B's line must not be in the note at all.
  await expect(document.locator('.note-block')).toHaveCount(1);
  await expect(document.locator('.note-block')).toContainText(`−${money(priceA)}`);
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
  await page.locator('#new-price').fill('31,00');
  await page.locator('#new-stock').fill('5');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.goto('/sales');
  await pickFromCatalog(page, name);
  await page.getByRole('button', { name: /Registrar venta/ }).click();

  // Same reason as above: selected by the id the draft reported (#52).
  const saleId = await recordedDocumentId(page);
  const doc = daySlip(page, saleId);
  await expect(doc).toHaveCount(1);
  await doc.click();
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
  await pickFromCatalog(page, name);

  // The line arrives priced; zeroing it is what makes it a regalo.
  const line = saleDraftLine(page);
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

/**
 * Each slip carries the balance it inherits, computed over the documents below it.
 *
 * S4 of `odd/tasks/receipt-pad-sales.md`: the running balance moved out of the
 * table's first column and into each slip's own head, and the assertion that used
 * to cover it went with the table. There was no e2e for the balance at all after
 * the migration — `running-balance.test.ts` proves the arithmetic, but nothing
 * proved the screen still *prints* it, which is the half a unit test cannot see.
 *
 * The test records two documents it owns and reads the balances of exactly those
 * two, by id. A balance is the sum of every document before it, so asserting on a
 * figure in a ledger every spec writes to would need the whole suite's arithmetic
 * — the same trap `recordedDocumentId` exists for (#52). Instead it asserts the
 * relationship the column exists to express: the newer of two consecutive
 * documents carries exactly the older one's balance plus its own total.
 */
test('each slip prints the balance it inherits from the documents below it', async ({ page }) => {
  const stamp = Date.now().toString(36);
  const name = `e2e-bal-${stamp}`;

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('10,00');
  await page.locator('#new-stock').fill('10');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.goto('/sales');
  await pickFromCatalog(page, name);
  const line = saleDraftLine(page);
  await line.locator('input[aria-label^="Cant"]').fill('2');
  await expect(page.locator('.sheet .total')).toHaveText(money('20,00'));
  await page.getByRole('button', { name: /Registrar venta/ }).click();
  const first = await recordedDocumentId(page);

  await pickFromCatalog(page, name);
  const lineAgain = saleDraftLine(page);
  await lineAgain.locator('input[aria-label^="Cant"]').fill('3');
  await expect(page.locator('.sheet .total')).toHaveText(money('30,00'));
  await page.getByRole('button', { name: /Registrar venta/ }).click();
  const second = await recordedDocumentId(page);

  const firstSlip = daySlip(page, first);
  const secondSlip = daySlip(page, second);
  await expect(firstSlip).toHaveCount(1);
  await expect(secondSlip).toHaveCount(1);

  // Read the numbers the screen printed, not numbers this test computed: if the
  // slip showed a bare total instead of an inherited balance, this comparison
  // fails instead of agreeing with the component.
  const read = async (text: string) => Number(text.replace(/[^\d,]/g, '').replace(',', '.'));
  const firstBalance = await read((await firstSlip.locator('.slip-bal').textContent()) ?? '');
  const secondBalance = await read((await secondSlip.locator('.slip-bal').textContent()) ?? '');

  // The newest slip must be exactly one sale above the one below it. That is the
  // whole claim of the column, and it is false for a slip that merely repeats its
  // own total — which is the failure mode the artboard's head invites.
  expect(Number.isFinite(firstBalance)).toBe(true);
  expect(Number.isFinite(secondBalance)).toBe(true);
  expect(secondBalance - firstBalance).toBeCloseTo(30, 2);
});
