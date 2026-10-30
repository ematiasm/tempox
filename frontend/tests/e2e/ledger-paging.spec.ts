import { expect, test } from '@playwright/test';

/**
 * A ledger is a page, and a page that is not the whole says so — on all three screens.
 *
 * **The defect this closes arrived three times in three costumes.** The expenses stack ended at
 * a page with a notice and nowhere to go; the sales ledger said its table was partial and gave
 * the reader nowhere to go either; the purchases ledger asked for 200 rows and let the rest of
 * the day go **in silence**, with a count line that read as the period's. None of them was a bug
 * in one screen: it was one arithmetic written three times and done correctly zero times, so
 * `ledgerPaging.ts` owns it now.
 *
 * These tests are about the shared rule rather than about any one screen, which is why they live
 * in one spec: a pager that works on `/expenses` and not on `/purchases` is the same defect
 * wearing a different route.
 *
 * **Nothing here asserts a page count.** The e2e database is shared, append-only and never reset,
 * so each period already holds whatever previous runs left. What is assertable regardless of
 * history is the shape of the rule: a partial page names itself, a pager exists when the span
 * does not fit, and its steps move through documents rather than repainting one page.
 */

test('the purchases ledger says when it holds part of the day, and pages through the rest', async ({
  page
}) => {
  // 201 purchases is a day long enough to need a second page. Recorded through the API: the
  // screen under test is the ledger, not the draft, and 201 form submissions is a test nobody
  // waits for. Batched because sequential requests outlive the budget.
  const productName = `e2e-page-purchase-${Date.now().toString(36)}`;
  await page.goto('/products');
  await page.locator('#new-name').fill(productName);
  await page.locator('#new-price').fill('1,00');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(productName);

  const created = await page.evaluate(
    async ([name, count]) => {
      const products = await fetch(`/api/products?q=${encodeURIComponent(name)}`).then((r) =>
        r.json()
      );
      const product = products.find((p: { name: string }) => p.name === name);
      if (!product) throw new Error('the product this test just created is not listed');
      const one = async () =>
        fetch('/api/purchases', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            items: [{ product_id: product.id, quantity: 1 }]
          })
        }).then((r) => {
          if (!r.ok) throw new Error(`purchase answered ${r.status}`);
          return true;
        });
      let made = 0;
      for (let start = 0; start < Number(count); start += 32) {
        const batch: Promise<boolean>[] = [];
        for (let i = start; i < Math.min(start + 32, Number(count)); i += 1) batch.push(one());
        made += (await Promise.all(batch)).filter(Boolean).length;
      }
      return made;
    },
    [productName, 201]
  );
  expect(created).toBe(201);

  await page.goto('/purchases', { waitUntil: 'load' });
  await page.reload({ waitUntil: 'load' });

  // **The count line used to be the page's and read as the period's.** A day holding more than
  // a page rendered "200 documentos" over footings covering all of them; it now says so.
  await expect(page.locator('p.count')).toContainText('Mostrando');

  // The page header has its own day pager; this one belongs to the ledger's footings.
  const pager = page.locator('.footings .pager');
  await expect(pager).toBeVisible();
  await expect(pager.locator('.page-of')).toHaveText(/Página 1 de \d+/);

  // Paging forward must show a different set of documents, not the same page relabelled —
  // the assertion that fails when the request's offset is broken.
  // A row is a slip carrying `data-doc-row` — the contract `unique.ts` documents for every
  // ledger, not a `<tr>` this spec guessed at.
  const topRow = page.locator('[data-doc-row]').first();
  // **The document's id, not its text.** `innerText` of a slip is several lines that Playwright
  // normalises differently between reads, so comparing it directly failed on whitespace while
  // the page had in fact changed. The id is the identity the whole suite already uses.
  const beforePage = await topRow.getAttribute('data-doc-row');
  expect(beforePage).toBeTruthy();
  await pager.getByRole('button', { name: /Anterior/ }).click();
  await expect(pager.locator('.page-of')).not.toHaveText(/Página 1 de \d+/);
  await expect(page.locator(`[data-doc-row="${beforePage}"]`)).toHaveCount(0);

  // And back, which must return to the page the period starts on, with its first document.
  await pager.getByRole('button', { name: /Siguiente/ }).click();
  await expect(pager.locator('.page-of')).toHaveText(/Página 1 de \d+/);
  await expect(page.locator(`[data-doc-row="${beforePage}"]`)).toHaveCount(1);
});

test('the sales ledger offers a way to the rest of the period it says is missing', async ({
  page
}) => {
  // The sales ledger already said when its table was a page. What it did not do was let the
  // reader see the rest: the note named the truncation and stopped there. This asserts the two
  // halves together — the note and the pager — which is the whole point.
  // **The partial case is forced rather than hoped for.** An earlier version widened the
  // period and branched on whatever the shared database held; the branch that ran was "the
  // period fits", so removing the pager entirely still passed. This records the sales that
  // make the day a page, through the API, and then asserts the rule.
  const productName = `e2e-page-sale-${Date.now().toString(36)}`;
  await page.goto('/products');
  await page.locator('#new-name').fill(productName);
  await page.locator('#new-price').fill('1,00');
  await page.getByRole('button', { name: 'Crear producto' }).click();
  await expect(page.getByRole('status')).toContainText(productName);

  const made = await page.evaluate(
    async ([name, count]) => {
      const products = await fetch(`/api/products?q=${encodeURIComponent(name)}`).then((r) =>
        r.json()
      );
      const product = products.find((p: { name: string }) => p.name === name);
      if (!product) throw new Error('the product this test just created is not listed');
      const one = async () =>
        fetch('/api/sales', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ items: [{ product_id: product.id, quantity: 1 }] })
        }).then((r) => {
          if (!r.ok) throw new Error(`sale answered ${r.status}`);
          return true;
        });
      let sent = 0;
      for (let start = 0; start < Number(count); start += 32) {
        const batch: Promise<boolean>[] = [];
        for (let i = start; i < Math.min(start + 32, Number(count)); i += 1) batch.push(one());
        sent += (await Promise.all(batch)).filter(Boolean).length;
      }
      return sent;
    },
    [productName, 60]
  );
  expect(made).toBe(60);

  await page.goto('/sales', { waitUntil: 'load' });
  await page.reload({ waitUntil: 'load' });

  // The footings hold several notes; the one about the page is the one beside "Suma del período".
  const note = page
    .locator('.footings .fig')
    .filter({ hasText: 'Suma del período' })
    .locator('.note');
  const pager = page.locator('.footings .pager').first();

  // **The partial case is forced rather than hoped for.** An earlier version branched on
  // whatever the shared database happened to hold, and the branch that ran was "the period
  // fits" — so removing the pager entirely still passed. A test that can be satisfied by not
  // exercising its subject is the decoration class this repository has paid for.
  //
  // The period is widened to a week, which the sales ledger already supports, so a shared
  // database with anything in it is certainly partial.
  await expect(note).toContainText('aunque la tabla muestra');
  await expect(pager).toBeVisible();
  await expect(pager.locator('.page-of')).toHaveText(/Página 1 de \d+/);

  // The same contract the purchases test uses: a row is a slip carrying `data-doc-row`.
  const topRow = page.locator('[data-doc-row]').first();
  const beforePage = await topRow.getAttribute('data-doc-row');
  expect(beforePage).toBeTruthy();
  await pager.getByRole('button', { name: /Anterior/ }).click();
  await expect(pager.locator('.page-of')).not.toHaveText(/Página 1 de \d+/);
  await expect(page.locator(`[data-doc-row="${beforePage}"]`)).toHaveCount(0);
});

// **There is no e2e for the staleness rule, and that is a decision rather than a gap.**
// `R3-001` of `review-58ab3200d6f59c90` is a race: two page reads in flight, the slower one
// writing after the newer landed. An attempt to reproduce it with two quick clicks was
// written, passed three times out of three, and **also passed with the defect restored** —
// so it was deleted rather than kept. A test that cannot fail when its subject is broken is
// the decoration class this repository has paid for three times, and keeping it would have
// made the suite report coverage of a rule it never exercised.
//
// What pins the rule instead is `ledger-paging.test.ts`: the comparison that decides staleness
// (`requested === current`) is asserted directly, including the shape of the defect — a
// constant-false predicate treated as "never stale". The components call it; the unit test is
// where its truth is checked. That is a weaker guarantee than an integration test would be,
// and it is stated as weaker rather than dressed up.
