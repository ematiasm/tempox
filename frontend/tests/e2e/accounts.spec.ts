import { expect, test } from '@playwright/test';

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
 * rule is counted here, over a fixed window, rather than inferred from timing.
 */

/** Counts the list reads the page makes, from the moment it is installed. */
function countListReads(page: import('@playwright/test').Page): string[] {
  const reads: string[] = [];
  page.on('request', (request) => {
    const url = new URL(request.url());
    // The path, not a prefix of the URL: `/api/accounts/:id` does not exist today,
    // and if it ever does, it is a different read with a different cost.
    if (request.method() === 'GET' && url.pathname === '/api/accounts') reads.push(request.url());
  });
  return reads;
}

test('the accounts screen reads the list once on mount, not in a loop', async ({ page }) => {
  const reads = countListReads(page);

  // The first response, not a rendered row: readiness here must not depend on what
  // the seeded database happens to hold.
  const firstRead = page.waitForResponse(
    (response) => new URL(response.url()).pathname === '/api/accounts' && response.status() === 200
  );
  await page.goto('/accounts');
  await firstRead;

  // A window after the screen is current. Under the loop this held hundreds of
  // requests — the defect was not a burst that settles, it was unbounded.
  await page.waitForTimeout(1000);

  // Exactly one. `toBeLessThanOrEqual(2)` would pass while the screen still read
  // twice, which is the shape of the bug rather than its absence: the write after
  // the fetch is what re-ran the effect.
  expect(reads).toHaveLength(1);
});

test('a write still re-reads the list, exactly once', async ({ page }) => {
  // The other half of the same rule, and the reason the fix is not simply "stop
  // reloading": the screen has no polling, so the *only* way it stays current after
  // a create, a rename, a promotion or a retirement is that the action re-reads.
  // A fix that removed the mount read would satisfy the count above and break this.
  const name = `e2e-once-${Date.now().toString(36)}`;
  const reads = countListReads(page);

  await page.goto('/accounts');
  const newAccount = page.locator('form.new input');
  await expect(newAccount).toBeVisible();
  await page.waitForTimeout(500);

  const beforeWrite = reads.length;

  await newAccount.fill(name);
  await page.getByRole('button', { name: 'Nueva cuenta' }).click();
  await expect(page.locator('li.row', { hasText: name })).toBeVisible();
  await page.waitForTimeout(500);

  expect(reads.length - beforeWrite).toBe(1);
});
