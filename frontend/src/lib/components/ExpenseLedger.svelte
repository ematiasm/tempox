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
  import { formatMoney, formatTime, parseAmount } from '#lib/money.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { fetchDay } from '#lib/api.js';

  interface Props {
    /** Bumped by the page when a document is recorded, so the day reloads. */
    refreshToken?: number;
    /** Reports how many documents the day holds, for the page's count. */
    oncount?: (count: number) => void;
  }

  let { refreshToken = 0, oncount }: Props = $props();

  let expenses = $state<Expense[]>([]);
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
   */
  async function load() {
    loading = true;
    error = null;
    try {
      const bounds = await fetchDay(new Date());
      shopZone = bounds.zone;
      // **Unscoped on purpose, and merged rather than shown twice** (`R3-003` of
      // `review-1e9c4c74ced6d94b`). The day list is scoped by `occurred_at`, so an owed
      // salary from an earlier day rendered no row — and with it, no advance form. The
      // feature's reason for existing was unreachable from the screen.
      //
      // One list, sorted by day: the day's documents first is what `listExpenses` already
      // orders by, and a second section would be a second place to keep in step.
      const [unsettled, foot] = await Promise.all([
        listExpenses({ limit: 200 }),
        getSpendingSummary({ from: bounds.from, to: bounds.to })
      ]);
      const inDay = new Set(
        unsettled
          .filter((row) => row.occurred_at >= bounds.from && row.occurred_at < bounds.to)
          .map((row) => row.id)
      );
      // Today's ledger, plus every older document that still owes — which is the only set
      // whose absence would hide an action the shop needs.
      expenses = unsettled.filter((row) => inDay.has(row.id) || row.outstanding !== '0.00');
      summary = foot;
      oncount?.(expenses.length);
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
