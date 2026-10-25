import { expect, test } from '@playwright/test';

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

  // **The whole row is the target**, and clicking it puts the id in the URL without
  // adding a history entry — so Back leaves the page rather than closing a panel.
  await row.getByRole('button').click();
  await expect(page).toHaveURL(/\?c=[0-9a-f-]{36}$/);
  await page.goBack();
  await expect(page).not.toHaveURL(/\?c=/);
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
