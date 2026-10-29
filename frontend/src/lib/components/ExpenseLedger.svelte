<script lang="ts">
  /**
   * The day's expenses, as a stack of slips.
   *
   * The composition the other two ledgers established is kept — a stamp, the document's
   * own line, and the figures the shop reads — because a third table that looked like a
   * spreadsheet would be a fourth idiom for the same job. What is new here is the pair of
   * figures each row carries: **`paid` and `outstanding`, both derived by the server**, so
   * this component renders them and never computes them. A second implementation of a
   * subtraction is how two numbers that should agree stop agreeing.
   */
  import {
    ApiError,
    advanceExpense,
    getSpendingSummary,
    listExpenses,
    type Expense
  } from '#lib/api.js';
  import {
    canStepPage,
    dayIsTruncated,
    dayOffset,
    dayPageCount,
    dayPageNumber,
    expensesInDay,
    mergeOlderDebts,
    unsettledQuery
  } from '#lib/expenseLedger.js';
  import { formatMoney, formatTime, parseAmount } from '#lib/money.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { fetchDay } from '#lib/api.js';

  interface Props {
    /** Bumped by the page when a document is recorded, so the day reloads. */
    refreshToken?: number;
    /** Reports how many documents the day holds, for the page's count. */
    oncount?: (count: number | null) => void;
  }

  /**
   * The day's own page, and it is **the size the baseline used**.
   *
   * The baseline asked for `limit: 200` and filtered to the day in the browser, so 200 was
   * how many of a day's documents reached the screen. Narrowing it to 50 while fixing the
   * older-debts question would have quietly shrunk what a busy day shows, and the notice
   * this component deserves for that case is the day-truncation one below the stack — which
   * only exists because the endpoint answers `cap` and `unsettled_older` (`R2-001` of
   * `review-16c84931d9274aaf`, measured: the earlier comment promised a foot notice that did
   * not exist, and `PAGE = 50` was a behaviour change nobody asked for).
   */
  const PAGE = 200;
  /**
   * The page asked for older debts.
   *
   * Larger than a day's page on purpose: this list is the shop's whole pending past, and
   * when it is not enough the count below says so rather than the screen pretending. 200 is
   * the endpoint's own cap, so asking for more would be clamped silently.
   */
  const OLDER_DEBTS_PAGE = 200;

  let { refreshToken = 0, oncount }: Props = $props();

  let expenses = $state<Expense[]>([]);
  /**
   * How many owed documents the server says exist **outside** the page it answered.
   *
   * Non-zero means the stack below is not the whole story, and saying so is the difference
   * between this fix and the one it replaces: the old screen could not tell a complete list
   * from a truncated one, so an owed document beyond its page was silently absent rather
   * than visibly pending (`R3-003` of `review-0e3269a3a9623ddc`, issue #115).
   */
  let olderOwedNotShown = $state(0);
  /**
   * How many of the day's own documents the server did not return.
   *
   * The day's list is a page too, and a page with no way to say "there is more" is the same
   * silent truncation the older-debts notice closes, one list over (`R2-001` of
   * `review-16c84931d9274aaf`). `cap` is the endpoint's own largest page, so `rows == cap` is
   * exactly the case where more may exist and the server said nothing more about it.
   */
  let dayNotShown = $state(0);
  /**
   * Which page of the day the stack is showing, as a row offset.
   *
   * A day can hold more documents than one page, and until this existed the notice below
   * the stack said so and left the operator nowhere to go. The offset is clamped by
   * `dayOffset` against the summary's exact `documents` count, so it can never address a
   * page that holds nothing (`R3-004` of `review-1e9c4c74ced6d94b`).
   */
  let dayPageOffset = $state(0);
  /** The day's exact document count, from the summary — not from the page that is shown. */
  let dayTotal = $state(0);
  /** The shop's zone, from `GET /api/day`. A browser's own clock is not the shop's. */
  let shopZone = $state<string | undefined>(undefined);
  let summary = $state<{
    expenses: string;
    purchases: string;
    supplier_returns: string;
    net: string;
  } | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let expanded = $state<string | null>(null);
  let advanceText = $state<string>('');
  let advancing = $state(false);
  let advanceError = $state<string | null>(null);

  /**
   * Loads the shop's day, then the documents in it and its spending foot.
   *
   * The bounds come from `GET /api/day` rather than from the browser's clock: the day is
   * the shop's, computed server-side from `SHOP_TIMEZONE`, which is what makes the ledger
   * and the documents the same day for a cashier whose machine is set elsewhere.
   *
   * **Two asks, and neither of them is a page hoping it is enough.** The day's own
   * documents come from a day-scoped list; the older documents that still owe come from the
   * server's `unsettled` filter, which is a question about documents rather than a bigger
   * `limit` (`R3-003` of `review-0e3269a3a9623ddc`, issue #115). The browser no longer
   * decides which documents are owed — it used to, over the newest 200 in the shop's whole
   * history, so an owed salary older than that page rendered no row and could never be paid
   * from the screen: the defect an earlier fix had already closed once, moved one page size
   * out, with nothing on screen to say anything was missing.
   */
  async function load() {
    loading = true;
    error = null;
    try {
      const bounds = await fetchDay(new Date());
      shopZone = bounds.zone;
      const [todays, older, foot] = await Promise.all([
        listExpenses({
          from: bounds.from,
          to: bounds.to,
          limit: PAGE,
          offset: dayPageOffset
        }),
        listExpenses(unsettledQuery(OLDER_DEBTS_PAGE)),
        getSpendingSummary({ from: bounds.from, to: bounds.to })
      ]);
      // The day first in the server's own order, then the older debts it cannot hold. Both
      // halves already arrive newest-first, so nothing is re-sorted here.
      expenses = mergeOlderDebts(todays.rows, older.rows);
      olderOwedNotShown = older.unsettled_older;
      // **The day's own page says when it is a page.** `unsettled_older` counts only what
      // owes, so a satisfied day would report zero no matter how much it truncated; the
      // endpoint's `cap` is the honest signal, and `PAGE` is now the baseline's 200 so this
      // notice is reachable only on a day nobody could read in one screen anyway.
      // The summary answered alongside this page, so its count is the day's own — exact,
      // and the address the pager clamps against.
      dayTotal = foot.documents;
      // **The page is re-validated against the count on every load, and that is not
      // bookkeeping.** A day grows and shrinks while the screen is open — a document recorded
      // in another tab, or this screen's own `refreshToken` reload after an advance — and a
      // remembered offset can end up past the new last page. Without this clamp the screen
      // would show an empty stack for a day whose foot reports documents, which is the
      // contradiction this whole slice exists to remove. Measured in the e2e: the component
      // kept its third page across a reload of a day that had shrunk to two.
      // The request above used the *old* offset, so when the clamp moves it the page on screen
      // is not the page that was asked for. Re-reading once is enough and cannot recurse: the
      // clamp is idempotent, so the second pass finds `clamped === dayPageOffset` and falls
      // through.
      const clamped = dayOffset(dayPageOffset, PAGE, dayTotal);
      if (clamped !== dayPageOffset) {
        dayPageOffset = clamped;
        await load();
        return;
      }
      dayNotShown = dayIsTruncated(todays.rows.length, dayTotal) ? 1 : 0;
      // **The day's count, not the merged list's.** The header sits above a foot that
      // reports the day's spending; counting the merge made a day with two expenses display
      // seven documents because five unrelated old debts were rendered under them.
      oncount?.(expensesInDay(todays.rows, bounds).length);
      summary = foot;
    } catch (e) {
      error = e instanceof ApiError ? e.message : lang.t.expenses_read_failed;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // `refreshToken` is the dependency: reading it here is what makes recording a document
    // reload the day, and the effect re-runs when the page bumps the token.
    void refreshToken;
    // Not `await`: an `$effect` is not an async function. `load` owns its own loading and
    // error state, so the effect only starts it.
    load();
  });

  async function recordAdvance(expense: Expense) {
    const amount = advanceText.trim() ? parseAmount(advanceText) : null;
    if (!amount || advancing) return;
    advancing = true;
    advanceError = null;
    try {
      await advanceExpense(expense.id, { amount });
      advanceText = '';
      await load();
    } catch (e) {
      // A `409` here is the ceiling talking, and its message names the number that did not
      // fit — surfacing it verbatim is better than a generic failure the cashier cannot act on.
      advanceError = e instanceof ApiError ? e.message : lang.t.expenses_write_failed;
    } finally {
      advancing = false;
    }
  }

  /**
   * Moves to the neighbouring page of the day, clamped by `dayOffset`.
   *
   * `stepPage` rather than assigning the offset directly, because the clamp is the rule that
   * keeps the pager from ever addressing a page that holds nothing.
   */
  function stepPage(delta: number) {
    const next = dayOffset(dayPageOffset + delta * PAGE, PAGE, dayTotal);
    if (next === dayPageOffset) return;
    dayPageOffset = next;
    // The stack is a new page: an expanded row from the previous one is not on screen any
    // more, and leaving `expanded` pointing at it would reopen a stranger's form on return.
    expanded = null;
    advanceText = '';
    advanceError = null;
    load();
  }

  function toggle(id: string) {
    expanded = expanded === id ? null : id;
    advanceText = '';
    advanceError = null;
  }
</script>

<section class="ledger" aria-label={lang.t.expenses_ledger_title}>
  {#if loading}
    <p class="state">{lang.t.expenses_loading}</p>
  {:else if error}
    <p class="state error" role="alert">{error}</p>
  {:else if expenses.length === 0}
    <p class="state">{lang.t.expenses_empty_day}</p>
  {:else}
    <ul class="slips">
      {#each expenses as expense (expense.id)}
        <li class="slip" class:open={expanded === expense.id}>
          <button
            class="row"
            type="button"
            onclick={() => toggle(expense.id)}
            aria-expanded={expanded === expense.id}
          >
            <span class="when figures">{formatTime(new Date(expense.occurred_at), shopZone)}</span>
            <span class="who">{expense.payee}</span>
            <span class="kind">{expense.category_name}</span>
            <span class="method">
              {expense.payment_method_name ?? lang.t.expenses_on_account}
            </span>
            <span class="amount figures">{formatMoney(expense.amount)}</span>
            <span class="owed figures" class:settled={expense.outstanding === '0.00'}>
              {expense.outstanding === '0.00'
                ? lang.t.expenses_paid
                : `${lang.t.expenses_outstanding} ${formatMoney(expense.outstanding)}`}
            </span>
          </button>

          {#if expanded === expense.id}
            <div class="detail">
              <dl class="figures-pair">
                <div>
                  <dt>{lang.t.expenses_paid}</dt>
                  <dd class="figures">{formatMoney(expense.paid)}</dd>
                </div>
                <div>
                  <dt>{lang.t.expenses_outstanding}</dt>
                  <dd class="figures">{formatMoney(expense.outstanding)}</dd>
                </div>
              </dl>

              {#if expense.payment_method_id === null && expense.outstanding !== '0.00'}
                <form
                  class="advance"
                  onsubmit={(event) => {
                    event.preventDefault();
                    recordAdvance(expense);
                  }}
                >
                  <label>
                    <span class="label">{lang.t.expenses_amount_advanced}</span>
                    <input bind:value={advanceText} inputmode="decimal" placeholder="0,00" />
                  </label>
                  <button type="submit" disabled={advancing}
                    >{lang.t.expenses_advance_action}</button
                  >
                </form>
                {#if advanceError}<p class="error" role="alert">{advanceError}</p>{/if}
              {:else if expense.payment_method_id === null}
                <p class="hint">{lang.t.expenses_advance_only_owed}</p>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  <!-- **A page that is not the whole says so.** The older-debts ask is capped like every
       other page, and a shop with more than a page of pending debts used to see the list as
       if it ended — the defect this fix exists for, one layer up. The debt itself is not
       lost: the answer names the count, so the figure here is a fact rather than an
       estimate, and the page is bounded so the count stays affordable. -->
  {#if olderOwedNotShown > 0}
    <p class="truncated" role="status">
      {lang.t.expenses_older_owed_notice(olderOwedNotShown)}
    </p>
  {/if}

  <!-- **The day's own page, and it is a different sentence.** The notice above is about debts
       this screen found and did not render; this one is about the day itself being longer
       than a page. Collapsing them would tell the operator "there are more debts" when the
       truth is "there are more documents", and only the second one means the figures below
       describe a day the stack does not show in full. Reachable only at the endpoint's cap
       (`R2-001` of `review-16c84931d9274aaf`). -->
  {#if dayNotShown || dayPageOffset > 0}
    <!-- **The pager, and it is the answer to the notice rather than a second complaint.**
         The notice above says part of the day is off screen; this says which page is shown
         and offers the way to the next one, so a busy day stops being a wall. It renders on
         any page but the first as well, because a reader on page three needs to know that
         and to get back. -->
    <!-- **The labels follow the ordering, not the offset.** The stack is newest-first and
         offset 0 is the newest page, so "Older" is the *forward* offset and "Newer" is back
         towards zero. The first version of this markup paired `stepPage(-1)` with "Older",
         which read correctly and did the opposite: pressing it moved towards today's newest
         documents while the button promised yesterday's, and on the first page it was the
         one that was disabled. Measured while writing the e2e, which is what caught it. -->
    <div class="pager">
      <button
        type="button"
        onclick={() => stepPage(1)}
        disabled={!canStepPage(dayPageOffset, PAGE, dayTotal, 1)}
      >
        ← {lang.t.expenses_page_older}
      </button>
      <span class="page-of figures">
        {lang.t.expenses_page_of(dayPageNumber(dayPageOffset, PAGE), dayPageCount(dayTotal, PAGE))}
      </span>
      <button
        type="button"
        onclick={() => stepPage(-1)}
        disabled={!canStepPage(dayPageOffset, PAGE, dayTotal, -1)}
      >
        {lang.t.expenses_page_newer} →
      </button>
    </div>
  {/if}

  {#if summary}
    <!-- The foot names its three terms rather than showing one netted figure: a shop that
         spent 300 on expenses and 500 on purchases is two facts, and a single total would
         destroy both. -->
    <dl class="foot">
      <div>
        <dt>{lang.t.expenses_spent}</dt>
        <dd class="figures">{formatMoney(summary.expenses)}</dd>
      </div>
      <div>
        <dt>{lang.t.expenses_purchases}</dt>
        <dd class="figures">{formatMoney(summary.purchases)}</dd>
      </div>
      <div>
        <dt>{lang.t.expenses_returned}</dt>
        <dd class="figures">{formatMoney(summary.supplier_returns)}</dd>
      </div>
      <div class="net">
        <dt>{lang.t.expenses_net}</dt>
        <dd class="figures">{formatMoney(summary.net)}</dd>
      </div>
    </dl>
  {/if}
</section>

<style>
  .ledger {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
  }

  .state {
    margin: 24px;
    font-size: 13px;
    color: var(--ink-soft);
  }

  .pager {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--rule);
  }
  .pager button {
    background: none;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    padding: 6px 12px;
    font: inherit;
    font-size: var(--step--1);
    color: var(--ink);
    cursor: pointer;
  }
  .pager button:disabled {
    color: var(--ink-soft);
    border-color: var(--rule);
    cursor: default;
  }
  .page-of {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }

  .truncated {
    margin: 0;
    padding: 10px 24px;
    font-size: var(--step--1);
    color: var(--ink-soft);
    border-bottom: 1px solid var(--rule);
  }

  .error {
    color: var(--accent);
  }

  .slips {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    min-height: 0;
  }

  .slip {
    border-bottom: 1px solid var(--rule);
  }

  .row {
    display: grid;
    grid-template-columns: 62px minmax(0, 1.4fr) minmax(0, 1fr) minmax(0, 0.9fr) auto auto;
    gap: 14px;
    align-items: baseline;
    width: 100%;
    padding: 12px 24px;
    background: none;
    border: none;
    font: inherit;
    text-align: left;
    color: inherit;
    cursor: pointer;
  }

  .row:hover {
    background: var(--ground);
  }

  .when,
  .method {
    font-size: 12px;
    color: var(--ink-soft);
  }

  .who {
    font-size: 13px;
  }

  .kind {
    font-size: 12px;
    color: var(--ink-soft);
  }

  .amount {
    font-size: 13px;
    text-align: right;
  }

  .owed {
    font-size: 12px;
    text-align: right;
    color: var(--accent);
  }

  .owed.settled {
    color: var(--ink-soft);
  }

  .detail {
    padding: 4px 24px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .figures-pair {
    display: flex;
    gap: 28px;
    margin: 0;
  }

  .figures-pair dt,
  .foot dt {
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-soft);
  }

  .figures-pair dd,
  .foot dd {
    margin: 2px 0 0;
    font-size: 13px;
  }

  .advance {
    display: flex;
    align-items: flex-end;
    gap: 10px;
  }

  .advance label {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .label {
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-soft);
  }

  .advance input {
    font: inherit;
    font-size: 13px;
    padding: 6px 8px;
    border: 1px solid var(--rule);
    border-radius: 2px;
    background: var(--surface);
    color: var(--ink);
    width: 120px;
  }

  .advance button {
    font: inherit;
    font-size: 12px;
    padding: 7px 12px;
    border: 1px solid var(--ink);
    border-radius: 2px;
    background: transparent;
    color: var(--ink);
    cursor: pointer;
  }

  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--ink-soft);
  }

  .foot {
    margin: auto 0 0;
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 18px;
    padding: 16px 24px;
    border-top: 1px solid var(--rule);
  }

  .foot .net dd {
    font-weight: 600;
  }
</style>
