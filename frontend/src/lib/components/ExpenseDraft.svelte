<script lang="ts">
  /**
   * The expense sheet: money leaving that touches no stock.
   *
   * **This is not `PurchaseDraft` with fewer fields, and the difference is the point.**
   * A purchase is goods and money together, so it has lines, snapshots and a supplier. An
   * expense has none of those: it is one amount, one category, one payee, and — the part
   * that makes it its own document — **a day it belongs to** that is not necessarily the
   * day it is paid. That is what lets a salary be entered in October and settled in
   * November.
   *
   * The payee is free text on purpose (`S2`): the parties an expense pays are mostly not
   * suppliers, and forcing Edenor into a purchase counterparty table would fill it with
   * rows that never sell anything and can never be deleted.
   */
  import {
    ApiError,
    createExpense,
    createExpenseCategory,
    listExpenseCategories,
    listPaymentMethods,
    type ExpenseCategory,
    type PaymentMethod
  } from '#lib/api.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { parseAmount } from '#lib/money.js';

  interface Props {
    /** Called after an expense is recorded, so the ledger can refresh. */
    onrecorded?: (expenseId: string) => void;
    /** Reports whether the sheet holds something recordable; the page owns the action. */
    onready?: (ready: boolean) => void;
  }

  let { onrecorded, onready }: Props = $props();

  let categories = $state<ExpenseCategory[]>([]);
  let methods = $state<PaymentMethod[]>([]);
  let categoryId = $state<string>('');
  let payee = $state<string>('');
  let amountText = $state<string>('');
  let methodId = $state<string>('');
  let occurredAt = $state<string>('');
  let submitting = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  /**
   * The id of the document this sheet just recorded.
   *
   * The ledger is append-only and shared with every other spec, so an e2e cannot find its
   * own expense by position or by a derived total — this repo paid for that twice
   * (`frontend/tests/e2e/unique.ts`). The id is the identity, and the banner renders it so
   * a spec can read it back.
   */
  let recordedId = $state<string | null>(null);
  let creatingCategory = $state(false);
  let newCategoryName = $state<string>('');
  let categoryError = $state<string | null>(null);

  /** Only active categories are offered; retired ones stay readable in the ledger. */
  $effect(() => {
    listExpenseCategories()
      .then((rows) => {
        categories = rows;
        // Default the picker to the first usable category rather than to empty, which
        // would leave the form unusable until the shop touched a field it did not need.
        if (!categoryId) categoryId = rows.find((row) => row.is_active)?.id ?? '';
      })
      .catch(() => (error = lang.t.expenses_read_failed));
    listPaymentMethods()
      .then((rows) => {
        methods = rows.filter((row) => row.is_active);
      })
      .catch(() => (error = lang.t.expenses_read_failed));
  });

  /**
   * The amount, parsed the way the shop types it.
   *
   * **The comma is the decimal separator in es-AR**, and the ledger's own formatter writes
   * `$ 1.234,56`; a sheet that only accepted `1234.56` would refuse exactly what the screen
   * beside it displays. `parseAmount` owns that grammar, so this does not re-derive it.
   */
  function parsedAmount(): string | null {
    const raw = amountText.trim();
    if (!raw) return null;
    // `parseAmount` already answers the API's amount string, normalised from the shop's
    // own format (`$ 1.234,56`). Converting to a number and back would be the rounding
    // this whole schema exists to avoid.
    return parseAmount(raw);
  }

  const amountOk = $derived(parsedAmount() !== null);
  const ready = $derived(Boolean(categoryId && payee.trim() && amountOk));

  $effect(() => {
    onready?.(ready);
  });

  /** Called by the page's action bar, which owns the screen's single next action. */
  export async function submit() {
    if (!ready || submitting) return;
    const amount = parsedAmount();
    if (!amount) return;

    submitting = true;
    error = null;
    notice = null;
    try {
      const created = await createExpense({
        category_id: categoryId,
        payee: payee.trim(),
        amount,
        // **Omitted, not `"on_account"`, is the till.** The two are different statements:
        // the first says money left, the second says the shop owes. The select's empty
        // value means "the till", and the explicit on-account option carries a sentinel.
        payment_method_id: methodId === '__on_account__' ? 'on_account' : methodId || undefined,
        occurred_at: occurredAt ? new Date(occurredAt).toISOString() : undefined
      });
      onrecorded?.(created.id);
      notice = lang.t.expenses_recorded;
      recordedId = created.id;
      // The payee and the amount clear; the category and the method stay, because the next
      // expense of a shift is usually the same kind through the same rail. The date clears
      // with them: it defaulted to today and the caller did not type it.
      payee = '';
      amountText = '';
      occurredAt = '';
    } catch (e) {
      error = e instanceof ApiError ? e.message : lang.t.expenses_write_failed;
    } finally {
      submitting = false;
    }
  }

  async function addCategory() {
    const name = newCategoryName.trim();
    if (!name || creatingCategory) return;
    creatingCategory = true;
    categoryError = null;
    try {
      const created = await createExpenseCategory(name);
      categories = [...categories, created];
      categoryId = created.id;
      newCategoryName = '';
    } catch (e) {
      categoryError = e instanceof ApiError ? e.message : lang.t.expenses_write_failed;
    } finally {
      creatingCategory = false;
    }
  }
</script>

<form
  class="sheet"
  onsubmit={(event) => {
    event.preventDefault();
    submit();
  }}
>
  <div class="sheet-head">
    <h2>{lang.t.expenses_title}</h2>
    <span class="tagline">{lang.t.expenses_tagline}</span>
  </div>

  <label class="field">
    <span class="label">{lang.t.expenses_payee}</span>
    <input
      bind:value={payee}
      placeholder={lang.t.expenses_payee_placeholder}
      autocomplete="off"
      required
    />
  </label>

  <div class="field">
    <label>
      <span class="label">{lang.t.expenses_category}</span>
      <select bind:value={categoryId} required>
        {#each categories.filter((c) => c.is_active) as category (category.id)}
          <option value={category.id}>{category.name}</option>
        {/each}
      </select>
    </label>
    {#if creatingCategory === false && newCategoryName === ''}
      <button type="button" class="link" onclick={() => (creatingCategory = true)}>
        {lang.t.expenses_category_new}
      </button>
    {:else}
      <div class="new-category">
        <input
          bind:value={newCategoryName}
          placeholder={lang.t.expenses_category_new}
          aria-label={lang.t.expenses_category_new}
        />
        <button type="button" class="link" disabled={creatingCategory} onclick={addCategory}>
          {lang.t.expenses_category_new_action}
        </button>
      </div>
      {#if categoryError}<span class="error">{categoryError}</span>{/if}
    {/if}
  </div>

  <label class="field">
    <span class="label">{lang.t.expenses_amount}</span>
    <input bind:value={amountText} inputmode="decimal" placeholder="0,00" required />
  </label>

  <label class="field">
    <span class="label">{lang.t.expenses_method}</span>
    <select bind:value={methodId}>
      <option value="">{lang.t.expenses_method_till}</option>
      {#each methods as method (method.id)}
        <option value={method.id}>{method.name}</option>
      {/each}
      <option value="__on_account__">{lang.t.expenses_on_account}</option>
    </select>
  </label>

  <label class="field">
    <span class="label">{lang.t.expenses_occurred_at}</span>
    <input type="date" bind:value={occurredAt} />
  </label>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if notice}
    <p class="notice" role="status" data-doc-id={recordedId}>{notice}</p>
  {/if}
</form>

<style>
  /* The sheet is a plain column of fields: an expense has no lines to lay out, so the
     composition that `/purchases` needs for its rows would be empty structure here. */
  .sheet {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 22px 24px 26px;
  }

  .sheet-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--rule);
  }

  .sheet-head h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }

  .tagline {
    font-size: 12px;
    color: var(--ink-soft);
  }

  .field {
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

  input,
  select {
    font: inherit;
    font-size: 13px;
    padding: 7px 9px;
    border: 1px solid var(--rule);
    border-radius: 2px;
    background: var(--surface);
    color: var(--ink);
  }

  input:focus-visible,
  select:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
  }

  .new-category {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }

  .link {
    align-self: flex-start;
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    font-size: 12px;
    color: var(--accent);
    text-decoration: underline;
    cursor: pointer;
  }

  .error {
    margin: 0;
    font-size: 12px;
    color: var(--accent);
  }

  .notice {
    margin: 0;
    font-size: 12px;
    color: var(--ink-soft);
  }
</style>
