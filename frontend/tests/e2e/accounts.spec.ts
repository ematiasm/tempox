import { expect, test, type Page, type Response } from '@playwright/test';

/**
 * How many times the accounts screen asks the server for the list.
 *
 * **This is a request count, and it exists because nothing else could see the
 * defect.** The screen's mount effect called an async loader whose first
 * statement read `accounts` *before* its first `await`:
 *
 * ```ts
 * async function reload() {
 *   if (accounts === null) reading = true;   // a tracked read, synchronously
 *   accounts = await listAccounts();         // the write that invalidated it
 * }
 * $effect(() => { void reload(); });
 * ```
 *
 * In Svelte 5 a `$state` read inside an `$effect` makes that state a dependency of
 * the effect, so the write after the fetch re-ran the effect that had started the
 * fetch. Measured on the development server before the fix, on a table holding two
 * accounts: **585 `GET /api/accounts` in 2.5 seconds**, and 624 in 3 — a closed
 * loop, not a slow screen. The `catch` retriggered the same way through
 * `accounts = null`.
 *
 * Neither existing level could have caught it. The unit suite never mounts a
 * Svelte component (jsdom is configured and unused, `odd/tasks/jsdom-unused-environment.md`),
 * and every e2e assertion in this directory is about what renders — which is
 * identical after one read and after six hundred. So the observable effect of the
 * rule is counted here, rather than inferred from what the page shows.
 *
 * **Every count below is anchored to a response, never to a wall clock.** The first
 * version of the second test took its baseline after `waitForTimeout(500)` and
 * subtracted, which reads a clock to answer a question the network already answers:
 * a mount read that arrived late would have been counted as part of the write. The
 * only `waitForTimeout` left is the one that asserts *nothing happens*, which is a
 * claim no event can make for you.
 */

/** The list read, as a path and a method: `/api/accounts/:id` does not exist today,
 * and if it ever does it is a different read with a different cost. The method is
 * part of the predicate because the write path posts to the same path — measured:
 * without it, `waitForResponse` resolved on the `POST` that created the account and
 * the assertion below compared a `201` against a `200`. */
function isListRead(response: Response): boolean {
  return (
    response.request().method() === 'GET' && new URL(response.url()).pathname === '/api/accounts'
  );
}

/** Counts the list reads the page issues, from the moment it is installed. */
function countListReads(page: Page): string[] {
  const reads: string[] = [];
  page.on('request', (request) => {
    if (request.method() === 'GET' && new URL(request.url()).pathname === '/api/accounts') {
      reads.push(request.url());
    }
  });
  return reads;
}

test('the accounts screen reads the list once on mount, not in a loop', async ({ page }) => {
  const reads = countListReads(page);

  // The first response, not a rendered row: readiness here must not depend on what
  // the seeded database happens to hold.
  const firstRead = page.waitForResponse(isListRead);
  await page.goto('/accounts');
  expect((await firstRead).status()).toBe(200);

  // Exactly one. `toBeLessThanOrEqual(2)` would pass while the screen still read
  // twice, which is the shape of the bug rather than its absence: the write after
  // the fetch is what re-ran the effect.
  expect(reads).toHaveLength(1);

  // The settle window, and the one place a clock belongs: the defect was not a
  // burst that finished, it was unbounded, so proving its absence means proving
  // that nothing else arrives after the screen is current. Measured, this held
  // hundreds of requests.
  await page.waitForTimeout(1000);
  expect(reads).toHaveLength(1);
});

test('a write still re-reads the list, exactly once', async ({ page }) => {
  // The other half of the same rule, and the reason the fix is not simply "stop
  // reloading": the screen has no polling, so the *only* way it stays current after
  // a create, a rename, a promotion or a retirement is that the action re-reads.
  // A fix that removed the mount read would satisfy the count above and break this.
  const name = `e2e-once-${Date.now().toString(36)}`;
  const reads = countListReads(page);

  // Both baselines are responses. The mount read is awaited before the count is
  // read, so a late one can never be attributed to the write.
  const mountRead = page.waitForResponse(isListRead);
  await page.goto('/accounts');
  expect((await mountRead).status()).toBe(200);
  const newAccount = page.locator('form.new input');
  await expect(newAccount).toBeVisible();
  expect(reads).toHaveLength(1);

  const writeRead = page.waitForResponse(isListRead);
  await newAccount.fill(name);
  await page.getByRole('button', { name: 'Nueva cuenta' }).click();
  expect((await writeRead).status()).toBe(200);
  await expect(page.locator('li.row', { hasText: name })).toBeVisible();

  // One read before the write, one after it. Stated as an absolute because the
  // baseline was an event: a delta would hide a wrong baseline, which is exactly
  // what the clock-based version did.
  expect(reads).toHaveLength(2);

  // And the write path does not loop either: it closes the same way the mount read
  // did, because it calls the same `reload()`.
  await page.waitForTimeout(500);
  expect(reads).toHaveLength(2);
});
