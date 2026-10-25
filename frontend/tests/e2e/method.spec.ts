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

/**
 * The debt account is seeded, and the screen must not offer to make it the till
 * (`odd/tasks/v3-seed-identities.md` S3, T5).
 *
 * The server refuses the promotion with a 409 since `0016` and the `accounts.rs`
 * pre-check, so this is not a safety net — it is the difference between a screen that
 * offers an impossible action and one that does not. Measured before this spec: the
 * button was rendered for `credit` and clicking it produced `accounts_write_failed (409)`
 * on a screen where nothing else had ever failed.
 *
 * Asserted by name, which is safe here for a reason the other specs do not have: a
 * second account called `credit` cannot exist — the seeded name is unique across the
 * table, and this spec creates nothing.
 */
test('only the till exists, because debt is a document and not an account', async ({ page }) => {
  // **This spec used to assert the opposite**: that the seeded `credit` account appeared as a
  // destination while the promotion button was withheld from it. `S7` of
  // `v4-settlement.md` removed that account from the initial schema — the owner's decision,
  // because nothing has shipped — so the screen has one destination, and the property worth
  // pinning is that the old row cannot come back through the UI.
  await page.goto('/accounts');

  await expect(page.locator('li.row', { hasText: 'credit' })).toHaveCount(0);

  const cashRow = page.locator('li.row', { hasText: 'cash' });
  await expect(cashRow).toHaveCount(1);
  await expect(cashRow).toContainText('Caja del negocio');
});

/**
 * A sale on account says so, in the ledger, instead of leaving the cell empty.
 *
 * `S2` of `v4-receivables.md`: a sale that moved no money has no method, and the
 * server answers `null`. Before this spec existed, the ledger interpolated that
 * `null` straight into a `<td>` — an empty column, which reads as a broken join or
 * a missing row rather than as the claim the document is making. The screen must
 * distinguish "on account" from "we could not resolve the rail".
 *
 * `walk-in` is refused for a sale on account (anonymous debt is not debt), so the
 * spec needs a customer, created through the API: the customer screen does not exist
 * yet, and inventing one here would be testing something else.
 */
test('a sale on account says so in the ledger, instead of an empty cell', async ({ page }) => {
  const customerName = `e2e-onaccount-${Date.now().toString(36)}`;
  const productName = `e2e-onaccount-prod-${Date.now().toString(36)}`;

  const customer = await page.request.post('/api/customers', {
    data: { name: customerName }
  });
  expect(customer.ok()).toBeTruthy();
  const { id: customerId } = (await customer.json()) as { id: string };

  const product = await page.request.post('/api/products', {
    data: { name: productName, list_price: '10.00' }
  });
  expect(product.ok()).toBeTruthy();
  const { id: productId } = (await product.json()) as { id: string };

  const sale = await page.request.post('/api/sales', {
    data: {
      customer_id: customerId,
      items: [{ product_id: productId, quantity: 1 }],
      payment_method_id: 'on_account'
    }
  });
  expect(sale.status()).toBe(201);
  const { id: saleId } = (await sale.json()) as { id: string };

  // The ledger row for that sale, found by identity — the same `aria-controls`
  // selector the other specs use, never by position or total (#52).
  await page.goto('/sales');
  const row = page.locator(`table.docs tbody tr.doc[aria-controls="lines-${saleId}"]`);
  await expect(row).toHaveCount(1);

  const cell = row.locator('td.method');
  await expect(cell).toHaveText('A cuenta');
  // The empty cell is the defect, and it is what this asserts against.
  await expect(cell).not.toBeEmpty();
  // And it is marked as a state rather than a rail, so the two are distinguishable
  // at a glance rather than only by reading the word.
  await expect(cell).toHaveClass(/on-account/);

  // **A real counter sale still names its rail**, so the screen did not simply lose
  // the column. This used to be `expect(price).toBe('10,00')` — a tautology over a
  // local constant that asserted nothing about the app, recorded by the review as
  // `R3-002`. It is now a sale through the API on the seeded `cash` method, found in
  // the ledger by identity, and it must render the method's name rather than the
  // on-account label.
  const { id: cashSaleId } = (await (
    await page.request.post('/api/sales', {
      data: { items: [{ product_id: productId, quantity: 1 }] }
    })
  ).json()) as { id: string };

  await page.reload();
  const cashRow = page.locator(`table.docs tbody tr.doc[aria-controls="lines-${cashSaleId}"]`);
  await expect(cashRow).toHaveCount(1);
  const cashCell = cashRow.locator('td.method');
  await expect(cashCell).toHaveText('cash');
  await expect(cashCell).not.toHaveClass(/on-account/);
});
