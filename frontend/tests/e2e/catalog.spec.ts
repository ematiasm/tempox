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

test('the create form exposes every field issue to assistive technology', async ({ page }) => {
  await page.goto('/products');

  const create = page.getByRole('button', { name: /Crear producto/ });
  const name = page.locator('#new-name');
  const price = page.locator('#new-price');
  const stock = page.locator('#new-stock');

  // Nothing typed yet. The button is disabled, and the reason is on the page:
  // the name and the price are each reported as missing, beside their own field.
  //
  // This is what replaces the old `.why` hint under the button, which said
  // "fill in the name and the price" no matter how many things were actually
  // wrong. Removing that hint is only safe while the messages carry the reason,
  // so a disabled button with no issue on screen is the regression to catch.
  await expect(create).toBeDisabled();
  await expect(page.locator('.warn')).toHaveCount(2);

  // The empty name is missing; the decimal point is invalid in the shop's
  // decimal-comma locale, and negative stock is independently invalid. All three
  // at once is the R2-001 shape: a single message chosen by a ternary could only
  // ever name one of them, leaving the other two red with nothing explaining why.
  await price.fill('25.00');
  await stock.fill('-1');
  await expect(name).toHaveAttribute('aria-invalid', 'true');
  await expect(price).toHaveAttribute('aria-invalid', 'true');
  await expect(stock).toHaveAttribute('aria-invalid', 'true');
  await expect(create).toBeDisabled();

  for (const [field, input, kind] of [
    ['name', name, 'missing'],
    ['price', price, 'invalid'],
    ['stock', stock, 'invalid']
  ] as const) {
    const messageId = await input.getAttribute('aria-describedby');
    expect(messageId).toBe(`new-${field}-message`);
    const message = page.locator(`#${messageId}`);
    await expect(message).toBeVisible();
    await expect(message).toHaveAttribute('data-issue-kind', kind);
  }

  // With valid numbers, the untouched name is the sole remaining issue and is
  // explicitly identified as missing rather than as an invalid value.
  await price.fill('25,00');
  await stock.fill('4');
  await expect(name).toHaveAttribute('aria-invalid', 'true');
  await expect(name).toHaveAttribute('aria-describedby', 'new-name-message');
  await expect(page.locator('#new-name-message')).toHaveAttribute('data-issue-kind', 'missing');
  await expect(price).not.toHaveAttribute('aria-invalid');
  await expect(stock).not.toHaveAttribute('aria-invalid');
  await expect(page.locator('.warn')).toHaveCount(1);
  await expect(create).toBeDisabled();

  // Typing the name clears the last issue and is the only thing that turns the
  // action on: no hint was hiding a blocker the messages do not report.
  await name.fill(`e2e-catalog-msg-${Date.now().toString(36)}`);
  await expect(page.locator('.warn')).toHaveCount(0);
  await expect(create).toBeEnabled();
});

test('a form submitted twice while a create is in flight sends one request', async ({ page }) => {
  const name = `e2e-catalog-dup-${Date.now().toString(36)}`;

  // The button disables itself while `creating`, and that is not what this test
  // measures. It measures the guard inside `create()`: the button is one caller,
  // and the form's submit event is another — Enter, a held key, an assistive
  // technology invoking submit, or a second event queued before the first render
  // lands. Only the guard covers all of them.
  //
  // Measured before this was written: with the guard reading only the parse
  // result, two dispatched submits produced TWO `POST /api/products` and two
  // products. The obvious click-based test passes anyway, because the disabled
  // button blocks the second click — the defect hides behind the very UI it
  // should not depend on.
  let posts = 0;
  await page.route('**/api/products', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    posts += 1;
    // Hold the response so the second submit lands while the first is in flight.
    await new Promise((resolve) => setTimeout(resolve, 600));
    await route.continue();
  });

  await page.goto('/products');
  await page.locator('#new-name').fill(name);
  await page.locator('#new-price').fill('25,00');
  await page.locator('#new-stock').fill('1');
  await expect(page.getByRole('button', { name: /Crear producto/ })).toBeEnabled();

  // Dispatch on the form itself, bypassing the disabled button on purpose: the
  // assertion is about the guard, and a test that can only fire through the
  // button cannot tell a guarded handler from an unguarded one.
  const submit = () =>
    page.evaluate(() => {
      document
        .getElementById('new-product')
        ?.dispatchEvent(new Event('submit', { cancelable: true, bubbles: true }));
    });

  await submit();
  await submit();

  await expect(page.getByRole('status')).toContainText(name);
  expect(posts).toBe(1);

  // And the catalog holds one row, not two: the second POST would have made a
  // duplicate the operator has no way to delete — the ledger is append-only.
  await page.locator('#catalog-search').fill(name);
  await expect(page.locator('table.catalog tbody tr', { hasText: name })).toHaveCount(1);
});
