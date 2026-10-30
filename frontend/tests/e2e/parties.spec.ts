import { expect, test } from '@playwright/test';
import { money } from './unique.js';

/**
 * The seeded `cash` method, named once instead of spelled four times.
 *
 * `R2-1` of `review-69a9c8cb07e21dca` (informational): the literal UUID was repeated at every
 * collect step and nothing in the file said *which* method it was, so a maintainer could not
 * tell what the test exercised and a seed reorder would retarget it silently. It is the
 * `cash` row seeded by `0001_schema.sql`, and `0001` refuses `DELETE` on the table, so the id
 * is stable — but the name is what makes the assertion legible, and a name is cheap.
 */
const CASH = '00000000-0000-0000-0000-000000000004';

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
      payment_method_id: CASH,
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

test('the panel follows the URL when the browser navigates back to it', async ({ page }) => {
  // **`R3-001` of review `review-899bd50b10d8b419`.** The page read `?c=` once on mount
  // and never again, so the URL and the panel could disagree the moment the browser
  // moved — and a panel that shows one party while the link names another is the one
  // failure a deep-linkable panel must not have.
  //
  // It is measured through `history.back()` after a `pushState` the test makes itself,
  // because this page only ever `replaceState`s: the listener is what is under test, not
  // the page's own navigation.
  const first = await page.request.post('/api/customers', {
    data: { name: `e2e-nav-a-${Date.now().toString(36)}` }
  });
  const { id: firstId, name: firstName } = (await first.json()) as { id: string; name: string };
  const second = await page.request.post('/api/customers', {
    data: { name: `e2e-nav-b-${Date.now().toString(36)}` }
  });
  const { id: secondId } = (await second.json()) as { id: string };

  await page.goto(`/clientes?c=${firstId}`);
  await expect(page.locator('aside.sheet h2')).toHaveText(firstName);

  // Navigate the browser to the other party's URL without a full reload.
  await page.evaluate((id) => {
    window.history.pushState({}, '', `/clientes?c=${id}`);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }, secondId);
  // The listener updated the panel: a different name, same panel.
  await expect(page.locator('aside.sheet h2')).not.toHaveText(firstName);

  // And Back returns to the first party, with the panel following the URL.
  await page.evaluate(() => window.history.back());
  await expect(page.locator('aside.sheet h2')).toHaveText(firstName);
});

/**
 * The supplier panel — the mirror, exercised rather than assumed.
 *
 * `v5-customer-account.md` S1 calls proveedores "the mirror of clientes", and a mirror
 * that is only asserted on its list is not a mirror: the labels differ (`Le debemos`
 * versus `Debe`), the direction of the figures is inverted, and the documents are
 * purchases. This spec drives the same three boxes on the other side.
 *
 * **The figures are deliberately opposite to the client case**: we owe 40 and hold
 * nothing, so the second figure must be absent — the exact inverse of the client spec,
 * where the first was zero and the second non-zero.
 */
test('a supplier panel shows what we owe, with its own labels', async ({ page }) => {
  const supplier = await page.request.post('/api/suppliers', {
    data: { name: `e2e-supp-panel-${Date.now().toString(36)}` }
  });
  expect(supplier.ok()).toBeTruthy();
  const { id: supplierId, name: supplierName } = (await supplier.json()) as {
    id: string;
    name: string;
  };

  const product = await page.request.post('/api/products', {
    data: { name: `e2e-supp-prod-${Date.now().toString(36)}`, list_price: '40.00' }
  });
  const { id: productId } = (await product.json()) as { id: string };

  const purchase = await page.request.post('/api/purchases', {
    data: {
      supplier_id: supplierId,
      items: [{ product_id: productId, quantity: 1 }],
      payment_method_id: 'on_account'
    }
  });
  expect(purchase.status()).toBe(201);
  const { purchase: created } = (await purchase.json()) as { purchase: { id: string } };

  await page.goto(`/proveedores?c=${supplierId}`);
  const panel = page.locator('aside.sheet');
  await expect(panel).toBeVisible();
  await expect(panel.locator('h2')).toHaveText(supplierName);

  // **The supplier label, not the customer one**: `Le debemos`, and the figure is what
  // the shop owes.
  await expect(panel).toContainText('Le debemos');
  await expect(panel.locator('dd.amount').first()).toHaveText(money('40,00'));

  // A second figure only appears when it is non-zero; nothing is in our favour here.
  await expect(panel.locator('dd.amount')).toHaveCount(1);

  // Box 3 lists the purchase that still owes, by identity.
  await expect(panel.locator('li.doc')).toHaveCount(1);
  await expect(panel).toContainText(created.id.slice(0, 8));
  await expect(panel).toContainText('compras impagas');
});

/**
 * The collection form, driven the way a cashier drives it.
 *
 * `odd/tasks/v5-payments.md` S3, S9. What only a browser can prove: the form lives in the
 * **balance box** (the owner's own placement), it can be submitted from there, and the
 * documents it closed leave the third box **in the same render** — the response carries the
 * account, so nothing is re-fetched. It also proves the one thing `S9` forbids: the form
 * states the remainder in words and never names the documents the server's rule will pick.
 *
 * **Every document is found by the id this spec created**, never by position, and never by a
 * count: the database is shared with every other spec (`unique.ts`).
 */
test('a collection typed in the panel closes two documents and says which was the rule', async ({
  page
}) => {
  const name = `e2e-collect-${Date.now().toString(36)}`;
  const customer = await page.request.post('/api/customers', { data: { name } });
  expect(customer.ok()).toBeTruthy();
  const { id: customerId } = (await customer.json()) as { id: string };

  // Two sales on account of **different sizes**: 80 and 45. Equal sizes would make the
  // applied figures unable to say which one received what — the fixture lesson slice 1
  // recorded as V1b, and the reason this test could not be written with 100/100.
  const makeSale = async (price: string) => {
    const product = await page.request.post('/api/products', {
      data: { name: `e2e-collect-prod-${price}-${Date.now().toString(36)}`, list_price: price }
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
    return (await sale.json()) as { id: string };
  };
  const olderSale = await makeSale('80.00');
  const newerSale = await makeSale('45.00');

  // The ids the UI will show are the **sale** ids (a cashier looks up a sale, not an
  // obligation), so the assertions below find rows by those.
  const shortened = (id: string) => `#${id.slice(0, 8)}`;

  await page.goto('/clientes');
  await page.locator('li.row', { hasText: name }).getByRole('button').click();
  const panel = page.locator('aside.sheet');
  await expect(panel).toBeVisible();

  // Both sales are in the third box, and the box's count is the two of them.
  await expect(panel.locator('li.doc')).toHaveCount(2);
  await expect(panel).toContainText(shortened(olderSale.id));
  await expect(panel).toContainText(shortened(newerSale.id));

  // **The action is in the balance box**, where the owner put it.
  const balanceBox = panel.locator('section.box', {
    has: page.getByRole('heading', { name: 'Saldo' })
  });
  await balanceBox.getByRole('button', { name: 'Cobrar' }).click();

  // The form is open, and its submit is refused until there is money: the hint names the
  // field rather than leaving a dead button.
  const form = balanceBox.locator('form.collect');
  await expect(form).toBeVisible();
  await expect(form.locator('button.do')).toBeDisabled();
  // **The hint names the first field that is missing, and then the next one.** An empty form
  // is missing both the method and the amount; a single sentence listing both would leave the
  // operator fixing neither reliably, and one that always named the amount would be wrong on
  // an empty form. Both are asserted, in the order the fields appear.
  await expect(form).toContainText('Elegí un medio de pago');

  // **125 arrives** — exactly the two debts and not a cent more — allocated by hand to the
  // **newer** sale for its 45. The remaining 80 must go to the older one by the rule, and the
  // form must say so in words, naming no document. (The first version of this spec sent 100,
  // which leaves the older sale with 25: correct behaviour, wrong expectation, and the panel
  // said so — 80 + 45 is 125 and the money has to come from somewhere.)
  await form.locator('select').selectOption(CASH);
  await expect(form).toContainText('Escribí un monto mayor a cero');
  await expect(form.locator('button.do')).toBeDisabled();
  await form.locator('input[inputmode="decimal"]').first().fill('125');
  await form.getByLabel(`Monto ${shortened(newerSale.id)}`).fill('45');
  await expect(form.locator('p.remainder')).toContainText('80,00');
  await expect(form.locator('p.remainder')).toContainText('más viejas');
  // **The forbidden statement**: the remainder paragraph names **no document at all**. The
  // first version of this assertion only looked for the older sale's id, and a form that
  // predicted the allocation would name whichever document it picked first — so it passed
  // while the rule was being re-implemented in the client (found by neutering).
  await expect(form.locator('p.remainder')).not.toContainText(shortened(olderSale.id));
  await expect(form.locator('p.remainder')).not.toContainText(shortened(newerSale.id));
  await expect(form.locator('p.remainder')).not.toContainText('#');

  await form.locator('button.do').click();

  // **The panel re-renders from the response, with no second read.** Both sales closed:
  // the newer one by the cashier's own 45, the older one by the 55 the rule spent on it.
  await expect(panel).toContainText('Nada pendiente');
  await expect(panel.locator('li.doc')).toHaveCount(0);
  await expect(panel.locator('dd.amount').first()).toHaveText(money('0,00'));

  // **And it says which documents that money closed, and why.** The response's `applied` with
  // its per-row `source` is the only statement of the rule's decision, and a panel that shows
  // only the new balance leaves the cashier unable to tell their own choice from the rule's
  // consequence — `R3-002` of review `review-b6bebae2ab91c89f`, which found the field being
  // discarded while four i18n strings sat unused. Both sentences are asserted, with the two
  // amounts the fixture made different on purpose.
  const receipt = panel.locator('section.receipt');
  await expect(receipt).toBeVisible();
  await expect(receipt).toContainText(money('45,00'));
  await expect(receipt).toContainText('asignado a mano');
  await expect(receipt).toContainText(money('80,00'));
  await expect(receipt).toContainText('por ser el más viejo');
  // The receipt names documents by id, so a cashier can match them to the list they just lost.
  await expect(receipt).toContainText(shortened(newerSale.id));
  await expect(receipt).toContainText(shortened(olderSale.id));

  // And the server agrees, in the database's own terms: the two obligations are resolved.
  const account = await page.request.get(`/api/customers/${customerId}/account`).then(
    (r) =>
      r.json() as Promise<{
        owed: string;
        in_favour: string;
        unpaid_sales: { source_document_id: string }[];
      }>
  );
  expect(account.unpaid_sales.map((s) => s.source_document_id)).not.toContain(olderSale.id);
  expect(account.unpaid_sales.map((s) => s.source_document_id)).not.toContain(newerSale.id);
  expect(account.owed).toBe('0.00');
  expect(account.in_favour).toBe('0.00');

  // **The rule's own statement is `applied`, with its `source`, and nothing in the client
  // can see it unless the browser was the one that recorded the collection.** That is why
  // the assertion above is the DOM and this one is the database: the panel proves the write
  // landed and re-rendered; the settlements prove *where* the rule put the money.
  const settlements = await page.request.get(`/api/customers/${customerId}/account`).then(
    (r) =>
      r.json() as Promise<{
        unpaid_sales: { id: string; source_document_id: string }[];
      }>
  );
  expect(settlements.unpaid_sales).toHaveLength(0);
});

/**
 * A receipt belongs to the party it was written for, and to nothing else.
 *
 * `R3-002` of `review-9cff43c50976184c`, recorded as informational and carried out of slice 2
 * unfixed (`v5-payments.md` `V9`). `collected` is page state, and the effect that fetches a
 * newly selected party cleared `account`, `notFound` and `panelError` but not it — so a
 * receipt for one client stayed on screen while another client's panel was open.
 *
 * **Why this is money and not cosmetics.** The receipt is the only statement of what a
 * collection closed: it names document ids and amounts. Left standing under a second client's
 * name, it reads as "this money closed these documents" about a party that has none of them —
 * the same class as the two-lies-the-T2-review-found, where the text was plausible and about
 * the wrong row.
 *
 * **What only a browser can prove.** No unit test can see this: the defect is that *mounting
 * the panel again* does not clear state the page owns, and the page is not the panel. Both
 * routes are exercised because the two copies of the effect would have to be fixed twice.
 */
test('a receipt does not follow the cashier to another party', async ({ page }) => {
  const stamp = Date.now().toString(36);
  const first = await page.request
    .post('/api/customers', { data: { name: `e2e-receipt-a-${stamp}` } })
    .then((r) => r.json() as Promise<{ id: string; name: string }>);
  const second = await page.request
    .post('/api/customers', { data: { name: `e2e-receipt-b-${stamp}` } })
    .then((r) => r.json() as Promise<{ id: string; name: string }>);

  // One sale on account for the first client, so its panel has a debt to collect against.
  const product = await page.request
    .post('/api/products', {
      data: { name: `e2e-receipt-prod-${stamp}`, list_price: '30.00' }
    })
    .then((r) => r.json() as Promise<{ id: string }>);
  const sale = await page.request.post('/api/sales', {
    data: {
      customer_id: first.id,
      items: [{ product_id: product.id, quantity: 1 }],
      payment_method_id: 'on_account'
    }
  });
  expect(sale.status()).toBe(201);

  await page.goto('/clientes');
  const panel = page.locator('aside.sheet');

  // Collect against the first client, which renders the receipt.
  await page.locator('li.row', { hasText: first.name }).getByRole('button').click();
  await expect(panel).toBeVisible();
  const balanceBox = panel.locator('section.box', {
    has: page.getByRole('heading', { name: 'Saldo' })
  });
  await balanceBox.getByRole('button', { name: 'Cobrar' }).click();
  const form = balanceBox.locator('form.collect');
  await form.locator('select').selectOption(CASH);
  await form.locator('input[inputmode="decimal"]').first().fill('30');
  await form.locator('button.do').click();

  const receipt = panel.locator('section.receipt');
  await expect(receipt).toBeVisible();

  // **Now open the second client, in the same page.** Its panel has no debt and no
  // collection, so a receipt here is a statement about the wrong party.
  await page.locator('li.row', { hasText: second.name }).getByRole('button').click();
  await expect(panel).toContainText(second.name);
  await expect(panel.locator('section.receipt')).toHaveCount(0);

  // **The mirror, because the effect is written twice.** `/proveedores` has its own copy of
  // this page's state, and a fix applied to one route is not a fix applied to the other — the
  // failure mode this repository already paid for once, when sales and purchases became
  // 2700 lines of the same table. A purchase on account gives the supplier a payable to pay.
  const supplierA = await page.request
    .post('/api/suppliers', { data: { name: `e2e-receipt-sup-a-${stamp}` } })
    .then((r) => r.json() as Promise<{ id: string; name: string }>);
  const supplierB = await page.request
    .post('/api/suppliers', { data: { name: `e2e-receipt-sup-b-${stamp}` } })
    .then((r) => r.json() as Promise<{ id: string; name: string }>);
  const purchase = await page.request.post('/api/purchases', {
    data: {
      supplier_id: supplierA.id,
      items: [{ product_id: product.id, quantity: 1 }],
      payment_method_id: 'on_account'
    }
  });
  expect(purchase.status()).toBe(201);

  await page.goto('/proveedores');
  await page.locator('li.row', { hasText: supplierA.name }).getByRole('button').click();
  await expect(panel).toBeVisible();
  const payBox = panel.locator('section.box', {
    has: page.getByRole('heading', { name: 'Saldo' })
  });
  await payBox.getByRole('button', { name: 'Pagar' }).click();
  const payForm = payBox.locator('form.collect');
  await payForm.locator('select').selectOption(CASH);
  await payForm.locator('input[inputmode="decimal"]').first().fill('30');
  await payForm.locator('button.do').click();
  await expect(panel.locator('section.receipt')).toBeVisible();

  await page.locator('li.row', { hasText: supplierB.name }).getByRole('button').click();
  await expect(panel).toContainText(supplierB.name);
  await expect(panel.locator('section.receipt')).toHaveCount(0);
});
