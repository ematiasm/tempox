import { expect, test } from '@playwright/test';
import { recordedDocumentId } from './unique';

/**
 * `/expenses` — money leaving that touches no stock.
 *
 * **This spec finds its document by identity, never by position.** The ledger is
 * append-only and shared with every other spec in the run, so `.first()` is "the newest
 * document" and is this spec's own only when nothing recorded one since — a lie that
 * passed locally for months and failed on CI (#52). The draft reports `data-doc-id` after
 * recording, and the row is located by that id.
 *
 * The total is not an identity either: a run-unique amount would still collide, and this
 * repo already paid twice for selecting a document by a derived figure
 * (`frontend/tests/e2e/unique.ts` documents both attempts).
 */

test('an expense is recorded and the ledger finds it by identity', async ({ page }) => {
  await page.goto('/expenses');
  await expect(page.getByRole('heading', { level: 1, name: 'Registrar gasto' })).toBeVisible();

  // A payee unique to this run, so the row is findable while debugging — but the
  // assertion below uses the id, not this string.
  const payee = `Ferreteria e2e ${Date.now()}`;

  await page.getByLabel('Pagado a').fill(payee);
  await page.getByLabel('Monto').fill('1234,56');

  await page.getByRole('button', { name: 'Registrar gasto', exact: true }).click();

  // The sheet reports the document it just wrote; that id is the identity.
  const id = await recordedDocumentId(page);

  // The ledger row for exactly that document.
  const row = page.locator('.slip', { has: page.locator(`[aria-expanded]`) }).filter({
    hasText: payee
  });
  await expect(row).toHaveCount(1);

  // **The figures are the shop's format, not `Number`'s.** Above a thousand, es-AR groups
  // with a dot: `$ 1.234,56`. A helper that printed `$ 1234,56` would fail only on large
  // runs — an intermittent failure hidden inside a fix for another one.
  await expect(row).toContainText('1.234,56');

  // The category the sheet defaulted to is rendered on the row: the name is joined by the
  // server, never snapshotted, so a rename corrects history instead of freezing it.
  await expect(row).toContainText('Alquiler');

  // And it is a paid document: the till is the default rail, so nothing is outstanding.
  await expect(row).toContainText('Pagado');

  // The id the draft reported really is the row's own document, and the assertion above
  // was not satisfied by a different expense that happens to share the payee.
  const opening = page.getByRole('button', { name: new RegExp(payee) });
  await opening.click();
  await expect(page.locator('.detail')).toBeVisible();
  // The detail names both figures, so the derived pair is on the screen and not only in
  // the API payload.
  await expect(page.locator('.detail')).toContainText('Pagado');
  await expect(page.locator('.detail')).toContainText('Debido');

  expect(id).toMatch(/^[0-9a-f-]{36}$/);
});

test('an owed expense takes an advance and the row reports what is left', async ({ page }) => {
  await page.goto('/expenses');

  const payee = `Sueldo e2e ${Date.now()}`;

  await page.getByLabel('Pagado a').fill(payee);
  await page.getByLabel('Monto').fill('300,00');
  // The explicit on-account option: money did not leave, the shop owes.
  await page.getByLabel('Pagado con').selectOption('__on_account__');
  await page.getByRole('button', { name: 'Registrar gasto', exact: true }).click();

  await recordedDocumentId(page);

  const row = page.locator('.slip').filter({ hasText: payee });
  await expect(row).toHaveCount(1);
  // An owed document shows what is outstanding from the first moment.
  await expect(row).toContainText('Debido');
  await expect(row).toContainText('300,00');

  // Open it and advance half.
  await row.getByRole('button', { expanded: false }).click();
  await page.getByLabel('Monto del adelanto').fill('100,00');
  await page.getByRole('button', { name: 'Registrar adelanto' }).click();

  // The day reloads after the advance, so the row now reports what is left — 200, not 300.
  const reloaded = page.locator('.slip').filter({ hasText: payee });
  await expect(reloaded).toContainText('200,00');
  await expect(reloaded).toContainText('Pagado');
});

test('an owed expense from an older day stays payable, and the header still counts the day', async ({
  page
}) => {
  await page.goto('/expenses');
  await expect(page.getByRole('heading', { level: 1, name: 'Registrar gasto' })).toBeVisible();

  // The day's own count, read before anything older exists. It is the figure the header
  // shows above a foot that reports the day's spending, so it must describe the day.
  const countBefore = await page.locator('.count-n').textContent();

  const payee = `Sueldo viejo e2e ${Date.now()}`;

  await page.getByLabel('Pagado a').fill(payee);
  await page.getByLabel('Monto').fill('300,00');
  await page.getByLabel('Pagado con').selectOption('__on_account__');
  // **A day in the past, which is the whole point.** An owed salary belongs to the month
  // it was earned, and the screen used to be scoped to the current shop day — so this
  // document rendered no row and, with it, no advance form: the feature's reason for
  // existing was unreachable (#115, `R3-003` of `review-1e9c4c74ced6d94b`).
  await page.getByLabel('El día al que pertenece').fill('2024-05-05');
  await page.getByRole('button', { name: 'Registrar gasto', exact: true }).click();

  await recordedDocumentId(page);

  // It is on the stack even though today is not its day. This is the assertion the bug
  // failed: the row was absent, silently, with nothing on screen to say anything was left
  // out.
  const row = page.locator('.slip').filter({ hasText: payee });
  await expect(row).toHaveCount(1);
  await expect(row).toContainText('Debido');
  await expect(row).toContainText('300,00');

  // **And the header still counts the day.** It used to count the merged list — the day
  // plus every older debt rendered under it — so a day with two expenses could display
  // seven documents over a foot that covered two.
  await expect(page.locator('.count-n')).toHaveText(countBefore ?? '0');

  // The action that was unreachable: open the older document and advance against it.
  await row.getByRole('button', { expanded: false }).click();
  await page.getByLabel('Monto del adelanto').fill('100,00');
  await page.getByRole('button', { name: 'Registrar adelanto' }).click();

  const reloaded = page.locator('.slip').filter({ hasText: payee });
  await expect(reloaded).toContainText('200,00');
});
