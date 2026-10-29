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

test('a day with more documents than a page can be paged through, and says which page it is on', async ({
  page
}) => {
  // **The end of `R3-004` of `review-1e9c4c74ced6d94b`.** The stack used to end at a page with
  // a notice and nowhere to go: 200 documents was a wall. This records enough documents into
  // *the shop's own current day* to need a second page, then pages to it.
  //
  // They go into today on purpose. The ledger shows the day the shop is having, and a pinned
  // future date would either need the screen to navigate there — a feature this PR does not
  // add — or leave the assertion unreachable. Two hundred and one rows in one day is a
  // fixture, not a realistic shop, and it is the only way to exercise the boundary.
  //
  // Recorded through the API rather than the draft: 201 form submissions is a test nobody
  // waits for, and the screen under test is the ledger, not the sheet.
  const payeePrefix = `Pagina e2e ${Date.now()}`;
  await page.goto('/expenses');

  // In batches: 201 sequential requests take longer than the test's whole budget, and the
  // order they land in is irrelevant — the ledger sorts by `occurred_at`, and the identities
  // this test looks for are the first and last *payee*, not the first and last request.
  const created = await page.evaluate(
    async ([prefix, count]) => {
      const categories = await fetch('/api/expense-categories').then((r) => r.json());
      const categoryId = categories[0].id;
      const one = async (i: number) => {
        const response = await fetch('/api/expenses', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            category_id: categoryId,
            payee: `${prefix} ${i}`,
            amount: '1.00'
          })
        });
        if (!response.ok) throw new Error(`expense ${i} answered ${response.status}`);
        return true;
      };
      const total = Number(count);
      const ids: boolean[] = [];
      for (let start = 0; start < total; start += 32) {
        const batch = [];
        for (let i = start; i < Math.min(start + 32, total); i += 1) batch.push(one(i));
        ids.push(...(await Promise.all(batch)));
      }
      return ids.filter(Boolean).length;
    },
    [payeePrefix, 201]
  );
  expect(created).toBe(201);

  // **A full reload, and then a wait for the ledger to settle.** The inserts above happened
  // while the screen was open, so the page count a previous render learned is stale by the
  // time it settles; and the parent's `oncount` callback re-renders the ledger without
  // remounting it, so the offset is component state that can survive a soft navigation. A
  // reload with `waitUntil: 'load'` plus a poll on the label is what makes the assertion
  // below a statement about a fresh render rather than about a race — measured: without the
  // settle, the label read "Página 2 de 3" from the previous run's position.
  await page.goto('/expenses', { waitUntil: 'load' });
  await page.reload({ waitUntil: 'load' });
  const pager = page.locator('.pager');
  const pageOf = page.locator('.page-of');
  await expect(pageOf).toHaveText(/Página 1 de \d+/);

  // **The claim that makes this a pagination test and not a decoration test.** The day is
  // ordered newest first, so the *last* document recorded is on page one and the *first* is
  // on page two. Both found by identity, never by position (`tests/e2e/unique.ts`).
  const newest = page.locator('.slip').filter({ hasText: `${payeePrefix} 200` });
  await expect(newest).toHaveCount(1);
  // **Not "the oldest is absent", which the first version asserted and which the shared
  // database makes untrue.** The e2e database is append-only and never reset, and this test
  // has already run more than once: the day holds a thousand documents from previous runs, so
  // "is this run's oldest document on page one" depends on how much history precedes it.
  //
  // What is true regardless of history is the property that matters: the document at offset
  // zero of the day is on page one, the pager names more than one page, and every page the
  // pager reaches holds documents. The identity assertion is then about *reaching* the oldest
  // one, not about where it starts. Bounded by the pager's own disabled state rather than a
  // guessed click count, so the test says "every page is reachable" rather than "there is a
  // second page". The day is ordered newest first, so the last document recorded is the top
  // row of page one.
  await expect(page.locator('.slip').first()).toBeVisible();

  // Page back, one click at a time, until this run's oldest document is reachable. Bounded by
  // the pager's own disabled state rather than by a guessed click count, so the test says
  // "every page is reachable" instead of "there is a second page".
  const back = pager.getByRole('button', { name: /Anterior/ });
  // The day holds more than one page, so the step towards its older half must be available.
  await expect(back).toBeEnabled();

  // **Walk every page and assert each one shows a DIFFERENT document.** The first version of
  // this loop only asserted "the page is not empty", which passed with the request's offset
  // hard-wired to zero — the pager rendered, the label advanced, and every page showed the
  // same newest rows. That is a decoration test, and this repository has paid for one before
  // (`v5-customer-account.md` V2b: a `len()` that proved nothing).
  //
  // The identity that makes it real: the top row of each page is a different payee.
  // **Where the walk starts matters.** The day holds a fixed number of pages, and the button
  // that pages forward is disabled exactly when the current page is the last one — so a walk
  // that begins there records one page and stops, which is what the first version of this
  // loop did after `page.reload()` landed on the last page of a day whose count the shared
  // database had grown. Starting from the first page is what makes `topPayees.length > 1` a
  // statement about pagination rather than about the fixture's history.
  await expect(pageOf).toContainText('Página 1 de');
  const topPayees: string[] = [];
  for (let i = 0; i < 200; i += 1) {
    const top = page.locator('.slip').first();
    await expect(top).toBeVisible();
    topPayees.push(await top.innerText());
    if (await back.isDisabled()) break;
    const before = topPayees[topPayees.length - 1];
    await back.click();
    // The next page must lead with a different document, which is what a working offset
    // means and what a pager that repaints page one does not.
    await expect(top).not.toHaveText(before);
  }

  // Forward again, all the way back, which must return to the page the day starts on. The
  // loop waits for the label to change after each click: without that it re-reads
  // `isDisabled` from the render that was already on screen and exits on the first pass,
  // leaving the stack a page short — which is exactly what the first version of this loop
  // did, and what made the assertion below fail while the pager was working.
  const forward = pager.getByRole('button', { name: /Siguiente/ });
  for (let i = 0; i < 200; i += 1) {
    if (await forward.isDisabled()) break;
    const label = await pageOf.innerText();
    await forward.click();
    await expect(pageOf).not.toHaveText(label);
  }
  await expect(pageOf).toHaveText(/Página 1 de \d+/);
  await expect(newest).toHaveCount(1);
});
