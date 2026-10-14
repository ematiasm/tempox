import { expect, test } from '@playwright/test';

// The shell, as the shop actually meets it: the app opens on the sales screen,
// the rail carries the two destinations, and the active one is the extended tab.
// `/notes` stays reachable by URL as the walking-skeleton proof (S5).

test('root redirects to the sales screen', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/sales$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Record a sale' })).toBeVisible();
});

test('rail carries both destinations and marks the active one', async ({ page }) => {
  await page.goto('/sales');
  const rail = page.getByRole('navigation', { name: 'Main' });
  const sales = rail.getByRole('link', { name: 'Sales' });
  const products = rail.getByRole('link', { name: 'Products' });

  await expect(sales).toBeVisible();
  await expect(products).toBeVisible();
  await expect(sales).toHaveAttribute('aria-current', 'page');
  await expect(products).not.toHaveAttribute('aria-current', 'page');

  // Navigation reads as navigation: the URL changes and the mark follows it.
  await products.click();
  await expect(page).toHaveURL(/\/products$/);
  await expect(products).toHaveAttribute('aria-current', 'page');
  await expect(sales).not.toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('heading', { level: 1, name: 'Products' })).toBeVisible();

  await sales.click();
  await expect(page).toHaveURL(/\/sales$/);
  await expect(sales).toHaveAttribute('aria-current', 'page');
});

test('the language toggle switches the shell without losing the route', async ({ page }) => {
  await page.goto('/sales');
  const rail = page.getByRole('navigation', { name: 'Main' });

  await expect(rail.getByRole('link', { name: 'Sales' })).toBeVisible();
  await rail.getByRole('button', { name: 'Switch language' }).click();

  // The dictionary changed and the route did not move.
  await expect(rail.getByRole('link', { name: 'Ventes' })).toBeVisible();
  await expect(page).toHaveURL(/\/sales$/);

  await rail.getByRole('button', { name: 'Switch language' }).click();
  await expect(rail.getByRole('link', { name: 'Sales' })).toBeVisible();
});

test('the notes demo stays reachable by URL but out of the rail', async ({ page }) => {
  await page.goto('/notes');
  await expect(page.getByRole('heading', { level: 1, name: 'Notes' })).toBeVisible();

  const rail = page.getByRole('navigation', { name: 'Main' });
  await expect(rail.getByRole('link', { name: /notes/i })).toHaveCount(0);
});
