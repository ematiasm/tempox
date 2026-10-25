import { expect, test } from '@playwright/test';
import { money } from './unique.js';

/**
 * The two party pages, end to end.
 *
 * `v5-customer-account.md` S1 and S7. What only a browser can prove here: the rail
 * offers the two destinations, each page reads its list from the real API, and a
 * row is a whole-row target that puts the id in the URL without pushing a history
 * entry per click (S2's `replaceState`, which is invisible to a unit test).
 *
 * **Nothing asserts a count of clients or suppliers.** The database is shared with
 * every other spec and both seeds are always present, so a length assertion says
 * nothing about this fixture — the same shape the T1 review recorded as `R3-002`.
 * Identity, always.
 */
test('the rail offers the two party pages and each reads its own list', async ({ page }) => {
  const rail = page.getByRole('navigation', { name: 'Principal' });

  await page.goto('/sales');
  await expect(rail.getByRole('link', { name: 'Clientes' })).toBeVisible();
  await expect(rail.getByRole('link', { name: 'Proveedores' })).toBeVisible();

  // A customer created through the API, found on the page by name.
  const name = `e2e-party-${Date.now().toString(36)}`;
  const created = await page.request.post('/api/customers', { data: { name } });
  expect(created.ok()).toBeTruthy();

  await rail.getByRole('link', { name: 'Clientes' }).click();
  await expect(page).toHaveURL(/\/clientes$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Clientes' })).toBeVisible();

  // The seeded walk-in is always there, and the new customer is there by name.
  const row = page.locator('li.row', { hasText: name });
  await expect(row).toHaveCount(1);

  // **The whole row is the target**, and clicking it selects that party in the URL.
  const before = page.url();

  // **`replaceState`, measured by the history length, not by pressing Back.**
  // `page.goBack()` was the first version of this assertion and it proved nothing:
  // after a `replaceState` there is no new entry to go back to, so Back leaves the
  // page for reasons unrelated to the property under test (review
  // `review-1817973d81dd8fda`, `R3-001`). The length is the direct measurement.
  // The length is read immediately before and after the click: `replaceState` leaves
  // it unchanged, `pushState` would add one. Comparing it to itself, as the first
  // version of this assertion did, measures nothing.
  const beforeLength = await page.evaluate(() => window.history.length);
  await row.getByRole('button').click();
  const afterLength = await page.evaluate(() => window.history.length);
  expect(afterLength).toBe(beforeLength);
  await expect(page).toHaveURL(/\?c=[0-9a-f-]{36}$/);
  await page.goto(before);
  await expect(page).not.toHaveURL(/\?c=/);

  // And the page reads the parameter it writes instead of only rewriting the URL:
  // a deep link to the seeded walk-in opens the panel. (T2 asserted a pending
  // paragraph here because the panel did not exist; T3 replaced it, so the assertion
  // moved to the panel itself rather than being deleted.)
  await page.goto(`${before}?c=00000000-0000-0000-0000-000000000001`);
  await expect(page.locator('aside.sheet')).toBeVisible();
});

test('a supplier is listed on its own page, and the two lists are not the same', async ({
  page
}) => {
  const supplierName = `e2e-supp-${Date.now().toString(36)}`;
  const created = await page.request.post('/api/suppliers', { data: { name: supplierName } });
  expect(created.ok()).toBeTruthy();

  await page.goto('/proveedores');
  await expect(page.getByRole('heading', { level: 1, name: 'Proveedores' })).toBeVisible();
  await expect(page.locator('li.row', { hasText: supplierName })).toHaveCount(1);

  // A supplier must NOT appear on the customers page: one endpoint each, and a page
  // that rendered the other list would still pass a "the row is there" assertion.
  await page.goto('/clientes');
  await expect(page.locator('li.row', { hasText: supplierName })).toHaveCount(0);
});

/**
 * The account panel, opened from a row and reachable by link.
 *
 * `v5-customer-account.md` S2 and S3. What only a browser can prove: the panel opens
 * on a click, the three boxes are there, a settled sale is absent from the third one,
 * and — the property that makes the deep link worth having — **`?c=<id>` reopens the
 * same panel from a cold load**, including when the id does not exist.
 */
test('a client panel opens from a row, shows both figures, and reopens by link', async ({
  page
}) => {
  const name = `e2e-panel-${Date.now().toString(36)}`;
  const customer = await page.request.post('/api/customers', { data: { name } });
  expect(customer.ok()).toBeTruthy();
  const { id: customerId } = (await customer.json()) as { id: string };

  const product = await page.request.post('/api/products', {
    data: { name: `e2e-panel-prod-${Date.now().toString(36)}`, list_price: '40.00' }
  });
  const { id: productId } = (await product.json()) as { id: string };

  // A sale on account for 40, and a collection of 60 which settles 40 and leaves 20
  // **in their favour** — the case a single netted figure would erase.
  const sale = await page.request.post('/api/sales', {
    data: {
      customer_id: customerId,
      items: [{ product_id: productId, quantity: 1 }],
      payment_method_id: 'on_account'
    }
  });
  expect(sale.status()).toBe(201);
  const { id: saleId } = (await sale.json()) as { id: string };

  const payment = await page.request.post('/api/payments', {
    data: {
      party_kind: 'customer',
      party_id: customerId,
      payment_method_id: '00000000-0000-0000-0000-000000000004',
      total: '60.00',
      direction: 'in'
    }
  });
  expect(payment.status()).toBe(201);
  const { id: paymentId } = (await payment.json()) as { id: string };

  // **The collection has to be applied**, or it is just 60 in their favour and the
  // sale still owes 40 — the first version of this fixture stopped here and the
  // second figure read 60 because nothing had been settled.
  // **The unpaid document carries two ids and they mean different things**: `id` is the
  // obligation and `source_document_id` is the sale. The first version of this lookup
  // matched `id` against the sale's id and found nothing, which is a distinction worth
  // keeping rather than papering over — a settlement names the obligation.
  const document = await page.request
    .get(`/api/customers/${customerId}/account`)
    .then(async (r) => {
      const body = (await r.json()) as {
        unpaid_sales: { id: string; source_document_id: string }[];
      };
      return body.unpaid_sales.find((s) => s.source_document_id === saleId);
    });
  if (!document) {
    // Print what the API actually answered rather than a bare 404 three lines later.
    const raw = await page.request.get(`/api/customers/${customerId}/account`);
    throw new Error(`the sale is not listed as unpaid; account was: ${await raw.text()}`);
  }
  const obligationId = document.id;
  const settled = await page.request.post('/api/settlements', {
    data: { obligation_id: obligationId, payment_id: paymentId, amount: '40.00' }
  });
  expect(settled.status(), await settled.text()).toBe(201);

  // **Settling the 40 in full removes the sale from the unpaid box**, which is `S4`'s
  // whole claim: a settled document is absent, and its absence is the state. The first
  // version of this assertion expected the sale to still be listed, which would have
  // been the opposite rule.
  const account = await page.request.get(`/api/customers/${customerId}/account`).then(
    (r) =>
      r.json() as Promise<{
        owed: string;
        in_favour: string;
        unpaid_sales: { source_document_id: string }[];
      }>
  );
  expect(account.unpaid_sales.map((s) => s.source_document_id)).not.toContain(saleId);
  expect(account.owed).toBe('0.00');
  expect(account.in_favour).toBe('20.00');

  await page.goto('/clientes');
  await page.locator('li.row', { hasText: name }).getByRole('button').click();

  const panel = page.locator('aside.sheet');
  await expect(panel).toBeVisible();
  await expect(panel.locator('h2')).toHaveText(name);

  // **Both figures, never netted**: nothing owed (the sale is settled) and 20 in their
  // favour (the surplus of the collection). A netted panel would show one number — and
  // here it would show the 20 as if the client owed it, which is the sign error the
  // server-side test `v5_a_fully_settled_sale_...` guards against.
  await expect(panel.locator('dd.amount').first()).toHaveText(money('0,00'));
  await expect(panel.locator('dd.amount').nth(1)).toHaveText(money('20,00'));

  // Box 3 is empty, because the only sale was settled: absence is the state (S4).
  await expect(panel.locator('li.doc')).toHaveCount(0);
  await expect(panel).toContainText('Nada pendiente');
  await expect(page).toHaveURL(/\?c=[0-9a-f-]{36}$/);

  // **The deep link is the point**: a cold load of that URL reopens the same panel.
  const linked = page.url();
  await page.goto('/sales');
  await page.goto(linked);
  await expect(page.locator('aside.sheet h2')).toHaveText(name);

  // Closing removes the parameter, so a refresh does not reopen what was closed.
  await page.locator('aside.sheet .close').click();
  await expect(page).not.toHaveURL(/\?c=/);
  await expect(page.locator('aside.sheet')).toHaveCount(0);

  // The payment is referenced so the collection above is not an unused fixture: it
  // is what puts 20 in their favour, and `settlements` apply it to the sale.
  expect(paymentId).toMatch(/^[0-9a-f-]{36}$/);
});

test('a deep link to a party that does not exist says so instead of showing zeros', async ({
  page
}) => {
  // **`S2`'s not-found state.** An empty account with zeros would read as "this
  // client owes nothing", which is a different and false statement.
  await page.goto('/clientes?c=00000000-0000-0000-0000-0000000000ee');
  const panel = page.locator('aside.sheet');
  await expect(panel).toBeVisible();
  await expect(panel).toContainText('No existe ese cliente o proveedor');
  // And it is not a balance: no figures are rendered for a party that is not there.
  await expect(panel.locator('dd.amount')).toHaveCount(0);
});
