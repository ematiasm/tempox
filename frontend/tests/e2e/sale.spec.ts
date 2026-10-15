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
  await page.locator('#new-price').fill('25.00');
  await page.locator('#new-stock').fill('10');
  await page.getByRole('button', { name: 'Create product' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  // The derived stock reads back, which proves the create actually landed.
  await page.locator('#catalog-search').fill(name);
  const row = page.locator('table.catalog tbody tr', { hasText: name });
  await expect(row).toBeVisible();
  await expect(row).toContainText('10');

  // 2. Record the sale: type-ahead, keyboard pick, quantity, live total.
  await page.goto('/sales');
  await page.getByRole('combobox', { name: 'Product' }).fill(name);
  const option = page.getByRole('option').first();
  await expect(option).toContainText(name);

  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');

  const line = page.locator('.sheet table.lines tbody tr').first();
  await expect(line).toContainText(name);
  // The price is prefilled from the list price and stays overridable.
  await expect(line.locator('input[aria-label^="Price"]')).toHaveValue('25.00');
  await expect(page.locator('.sheet .total')).toHaveText('$25.00');

  await line.locator('input[aria-label^="Qty"]').fill('4');
  await expect(page.locator('.sheet .total')).toHaveText('$100.00');

  // 3. Record it. The band's action is the page's single next action.
  await page.getByRole('button', { name: /Record sale/ }).click();

  // 4. The stamp, which is what this barrier exists to protect.
  const recorded = page.locator('.recorded');
  await expect(recorded).toContainText('Confirmed');
  await expect(recorded).toContainText('$100.00');
  await expect(page.locator('.sheet table.lines tbody tr')).toHaveCount(0);

  // 5. And it is in the day's ledger, stamped, with its lines behind it.
  const doc = page.locator('table.docs tbody tr.doc').first();
  await expect(doc).toContainText('Walk-in — Consumidor Final');
  await expect(doc).toContainText('$100.00');
  await expect(doc.locator('.stamp')).toContainText('Confirmed');

  await doc.locator('.doc-id').click();
  await expect(page.locator('.lines table')).toContainText(name);
  await expect(page.locator('.lines table')).toContainText('$100.00');
});

test('a bonus line is recorded and moves no money', async ({ page }) => {
  const name = `e2e-gift-${Date.now().toString(36)}`;

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('10.00');
  await page.locator('#new-stock').fill('5');
  await page.getByRole('button', { name: 'Create product' }).click();
  await expect(page.getByRole('status')).toContainText(name);

  await page.goto('/sales');
  await page.getByRole('combobox', { name: 'Product' }).fill(name);
  await expect(page.getByRole('option').first()).toContainText(name);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');

  // The line arrives priced; zeroing it is what makes it a regalo.
  const line = page.locator('.sheet table.lines tbody tr').first();
  await line.locator('input[aria-label^="Price"]').fill('0.00');
  await expect(page.locator('.sheet .total')).toHaveText('$0.00');
  // The sheet says what that line is, so a zero total is never a silent bug.
  await expect(page.locator('.sheet')).toContainText('bonus line');

  await page.getByRole('button', { name: /Record sale/ }).click();
  await expect(page.locator('.recorded')).toContainText('Confirmed');

  // The stock still left the shop: that is the whole point of recording it.
  await page.goto('/products');
  await page.locator('#catalog-search').fill(name);
  const row = page.locator('table.catalog tbody tr', { hasText: name });
  await expect(row).toBeVisible();
  await expect(row).toContainText('4');
});
