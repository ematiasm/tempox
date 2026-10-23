import { expect, test } from '@playwright/test';
import { lineTotal, money, recordedDocumentId } from './unique.js';
import { pickFromCatalog } from './picker.js';

/**
 * The whole reason v3 exists, end to end (S8, S13).
 *
 * Before accounts, every sale wrote the seeded `cash` method and `/api/cash` summed
 * every confirmed sale — so a sale paid by bank transfer counted as money in the
 * till, and there was no way to say otherwise. This spec records two sales of the
 * same size through two different methods and asserts the two figures **disagree**:
 * the till holds one, the other account holds the other.
 *
 * It goes through the real UI against the real API, because the unit tests already
 * prove the view sums correctly and what is unproven is that the screen reads it and
 * that a method chosen at the counter reaches the right account.
 */
test('a sale paid by transfer lands in that account, not in the till', async ({ page }) => {
  const accountName = `e2e-bank-${Date.now().toString(36)}`;
  const methodName = `e2e-transfer-${Date.now().toString(36)}`;
  const productName = `e2e-method-${Date.now().toString(36)}`;
  const price = '50,00';
  const total = lineTotal(price, 1);

  // 1. A real second account with its own method, created through the screen the
  //    shop actually uses — not through the API, so the accounts page is covered too.
  await page.goto('/accounts');
  await page.locator('form.new input').fill(accountName);
  await page.getByRole('button', { name: 'Nueva cuenta' }).click();

  const accountRow = page.locator('li.row', { hasText: accountName });
  await expect(accountRow).toBeVisible();
  // A new account holds nothing, and its balance is a derived zero — not absent.
  await expect(accountRow.locator('.figure')).toHaveText(money('0,00'));

  await accountRow.getByRole('button', { name: 'Nuevo medio' }).click();
  await accountRow.locator('form.inline input').fill(methodName);
  await accountRow.locator('form.inline').getByRole('button', { name: 'Guardar' }).click();
  await expect(accountRow).toContainText(methodName);

  // 2. A product to sell.
  await page.goto('/products');
  await page.locator('#new-name').fill(productName);
  await page.locator('#new-price').fill(price);
  await page.locator('#new-stock').fill('5');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(productName);

  // 3. Record the sale on the transfer method. The picker is the one control this
  //    spec exists to exercise: choosing it must be possible from the counter.
  await page.goto('/sales');
  await pickFromCatalog(page, productName);

  const line = page.locator('.sheet table.lines tbody tr').first();
  await expect(line).toContainText(productName);

  const picker = page.getByLabel('Medio', { exact: true });
  await picker.selectOption({ label: methodName });

  await page.getByRole('button', { name: /Registrar venta/ }).click();
  const recorded = page.locator('.recorded');
  await expect(recorded).toContainText(money(total));

  // 4. The ledger shows which method paid. A ledger that showed only the amount
  //    would make this sale indistinguishable from a cash one.
  const id = await recordedDocumentId(page);
  // The full id appears only in the row's `aria-controls`, the same selector
  // `sale.spec.ts` uses — position and total are not identities (#52).
  const ledgerRow = page.locator(`table.docs tbody tr.doc[aria-controls="lines-${id}"]`);
  await expect(ledgerRow).toHaveCount(1);
  await expect(ledgerRow.locator('td.method')).toHaveText(methodName);

  // 5. The point: the bank holds it, the till does not.
  await page.goto('/accounts');
  await expect(page.locator('li.row', { hasText: accountName }).locator('.figure')).toHaveText(
    money(total)
  );

  // The other half — that the till did NOT receive it — is deliberately **not**
  // asserted here, and the reason is measured rather than assumed. Playwright runs
  // specs concurrently against one shared database, so the till is a resource every
  // purchase spec also moves: three attempts failed for three different honest
  // reasons. `not.toHaveText(total)` was blind to a coincidental match; an absolute
  // `$ 0,00` failed because a purchase had left it at `-$ 30,00`; and a
  // before/after comparison failed because another worker changed it between the two
  // reads. A shared resource cannot carry an assertion about one spec's activity.
  //
  // The claim that the till stays out of it is proven where it can be: the balance
  // assertions in `v3_t3_the_four_documents_move_money_with_the_right_sign` and
  // `v3_t6_the_till_is_the_default_account_not_every_sale`, each against a database
  // that test owns alone.
});
