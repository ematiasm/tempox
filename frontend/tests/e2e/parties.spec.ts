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
  // the earlier version claimed the click was wired while nothing consumed it.
  await page.goto(`${before}?c=00000000-0000-0000-0000-000000000001`);
  await expect(page.getByRole('status')).toBeVisible();
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
