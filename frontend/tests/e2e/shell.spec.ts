import { expect, test } from '@playwright/test';

// The shell, as the shop actually meets it: the app opens on the sales screen in
// the shop's own language (es-AR, S2), the rail carries the two destinations, and
// the active one is the extended tab. `/notes` stays reachable by URL as the
// walking-skeleton proof (S5).

test('root redirects to the sales screen', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/sales$/);
  await expect(page.getByRole('heading', { level: 1, name: 'Registrar venta' })).toBeVisible();
});

test('the document declares the shop language before any script runs (S14)', async ({ page }) => {
  // The attribute is server-rendered in `app.html`, so it is already correct on
  // the very first byte: a screen reader does not have to wait for hydration to
  // announce Spanish text with a Spanish voice.
  await page.goto('/sales');
  await expect(page.locator('html')).toHaveAttribute('lang', 'es-AR');
});

test('rail carries both destinations and marks the active one', async ({ page }) => {
  await page.goto('/sales');
  const rail = page.getByRole('navigation', { name: 'Principal' });
  const sales = rail.getByRole('link', { name: 'Ventas' });
  const products = rail.getByRole('link', { name: 'Productos' });

  await expect(sales).toBeVisible();
  await expect(products).toBeVisible();
  await expect(sales).toHaveAttribute('aria-current', 'page');
  await expect(products).not.toHaveAttribute('aria-current', 'page');

  // Navigation reads as navigation: the URL changes and the mark follows it.
  await products.click();
  await expect(page).toHaveURL(/\/products$/);
  await expect(products).toHaveAttribute('aria-current', 'page');
  await expect(sales).not.toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('heading', { level: 1, name: 'Productos' })).toBeVisible();

  await sales.click();
  await expect(page).toHaveURL(/\/sales$/);
  await expect(sales).toHaveAttribute('aria-current', 'page');
});

test('the language toggle switches the whole shell without losing the route', async ({ page }) => {
  await page.goto('/sales');
  // Scoped by the element, not by its accessible name: the rail's `aria-label`
  // is itself a translated string, so a name-based locator would stop resolving
  // at the moment the language changes -- which is the thing under test.
  const rail = page.locator('nav.rail');

  await expect(rail.getByRole('link', { name: 'Ventas' })).toBeVisible();
  // The toggle offers the *other* language, so it reads EN while es-AR is active.
  await rail.getByRole('button', { name: 'Cambiar idioma' }).click();

  // The dictionary changed, the document language followed it, and the route did
  // not move (S2, S14).
  await expect(rail.getByRole('link', { name: 'Sales' })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page).toHaveURL(/\/sales$/);
  // Money follows the same switch: the same total, the other shape (S4).
  await expect(page.locator('.sheet .total')).toContainText('$');

  await rail.getByRole('button', { name: 'Switch language' }).click();
  await expect(rail.getByRole('link', { name: 'Ventas' })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('lang', 'es-AR');
});

test('the notes demo stays reachable by URL but out of the rail', async ({ page }) => {
  await page.goto('/notes');
  await expect(page.getByRole('heading', { level: 1, name: 'Notas' })).toBeVisible();

  const rail = page.getByRole('navigation', { name: 'Principal' });
  await expect(rail.getByRole('link', { name: /notas/i })).toHaveCount(0);
});
