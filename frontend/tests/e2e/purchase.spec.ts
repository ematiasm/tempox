import { expect, test } from '@playwright/test';
import { money, recordedDocumentId } from './unique.js';
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
  //    test also covers that screen. The stock field is LEFT ALONE on purpose:
  //    an empty initial stock is 0, while typing `0` is refused — a quantity
  //    must be at least 1 — and the form then disables its own create button.
  //    Measured while writing this test: filling `0` hangs the click for the
  //    full 30s timeout instead of failing where the reader can see why.
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

  const line = page.locator('.sheet table.lines tbody tr').first();
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
  await expect(page.locator('.sheet table.lines tbody tr')).toHaveCount(0);
  const purchaseId = await recordedDocumentId(page);

  // 5. It is in the day's ledger with its lines behind it. Selected by the id the
  //    draft reported: position is a lie in a shared ledger and a total is not
  //    unique (#52).
  const doc = page.locator(`.ledger tr.doc[aria-controls="purchase-lines-${purchaseId}"]`);
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
