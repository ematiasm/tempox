import { expect, test } from '@playwright/test';

/**
 * The catalog's create form, measured where an operator meets it.
 *
 * Both specs exist because the form refuses work in ways no other level can see.
 * The refusal is not an error the API writes: the screen disables its own button
 * and prints a hint, so there is no status code to assert on and no log line. A
 * unit test can prove a parser right while the form still hangs the click.
 *
 * Every run uses a product name unique to it, so neither spec depends on what a
 * previous run left behind.
 */

test('a product can be created with an initial stock of zero', async ({ page }) => {
  const name = `e2e-catalog-zero-${Date.now().toString(36)}`;

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('25,00');
  // Typed, not left blank: zero is what the shelf holds before the goods
  // arrive, and a field the shop reads as a number has to accept the number.
  await page.locator('#new-stock').fill('0');

  // The form's refusal is the whole failure, and it is invisible to the API: a
  // disabled button means nothing was sent. Asserting the button is enabled is
  // what catches it, because a click on a disabled control waits out the whole
  // test timeout and then reports nothing about why.
  const create = page.getByRole('button', { name: /Crear producto/ });
  await expect(create).toBeEnabled();

  await create.click();
  await expect(page.getByRole('status')).toContainText(name);

  // Stock is derived from the movement ledger, so reading it back proves the
  // create landed with the row it claims, not only that a banner appeared.
  await page.locator('#catalog-search').fill(name);
  const row = page.locator('table.catalog tbody tr', { hasText: name });
  await expect(row).toBeVisible();
  await expect(row.locator('.stock')).toHaveText('0');
});

test('the create form names the field that is wrong', async ({ page }) => {
  await page.goto('/products');
  await page.locator('#new-name').fill(`e2e-catalog-msg-${Date.now().toString(36)}`);
  await page.locator('#new-stock').fill('4');

  // The shop writes amounts with a decimal comma; `25.00` is the wire form the
  // API answers with, and pasting it into this field is the mistake to name.
  await page.locator('#new-price').fill('25.00');
  await expect(page.locator('#new-price')).toHaveClass(/bad/);
  await expect(page.locator('.warn')).toContainText('precio');

  // And the same message names the stock when the stock is the wrong one. One
  // sentence for both fields was the defect: with the name and the price filled
  // in, it claimed they were missing.
  await page.locator('#new-price').fill('25,00');
  await expect(page.locator('#new-price')).not.toHaveClass(/bad/);
  await page.locator('#new-stock').fill('-1');
  await expect(page.locator('#new-stock')).toHaveClass(/bad/);
  await expect(page.locator('.warn')).toContainText('stock');
});
