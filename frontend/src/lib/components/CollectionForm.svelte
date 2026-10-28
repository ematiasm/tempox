<script lang="ts">
  /**
   * The collection form: money that arrived, and the debts it answers.
   *
   * `odd/tasks/v5-payments.md` S9, S10. The owner put it in the panel's **balance box** —
   * *"el segundo el saldo y ahi el boton para hacentar el pago"* — so this renders inside box
   * 2, under the two figures, and the form closes over the documents box's rows that it
   * annotates with an amount each.
   *
   * **It never simulates the allocation rule** (`S9`). The remainder is shown as a figure and
   * a sentence, and it never names which documents it will close: that answer belongs to the
   * server's `applied`, with its `source`, and a second implementation of one rule in a client
   * is how the two disagree. This repository paid for that class three times in one slice
   * (`V7b` of `v5-customer-account.md`).
   *
   * **The submit is the primary action and it is an ink bar, never the accent** (`S10`,
   * `DESIGN.md`'s "The Balance Owns the Accent Rule"): the accent stays on the balance figure,
   * which is the loudest thing in the box, and a form's own bar must not compete with it.
   */
  import { lang } from '#lib/i18n/index.svelte.js';
  import { formatMoney } from '#lib/money.js';
  import {
    allocationPreview,
    canSubmitCollection,
    collectionRequest,
    type CollectionBlocker,
    type CollectionDraft
  } from '#lib/collectionForm.js';
  import {
    collectFromCustomer,
    listPaymentMethods,
    paySupplier,
    type Collection,
    type PaymentMethod
  } from '#lib/api.js';

  let {
    side,
    accountId,
    unpaid,
    onrecorded,
    oncancel
  }: {
    side: 'customer' | 'supplier';
    /** The party's id — the account's own, so this component has one source for it. */
    accountId: string;
    /** The unpaid documents, exactly as the account payload listed them. */
    unpaid: { id: string; sourceDocumentId: string; total: string; outstanding: string }[];
    /** The response, handed to the page so the panel replaces its account in one snapshot. */
    onrecorded: (collection: Collection) => void;
    oncancel: () => void;
  } = $props();

  let methodId = $state('');
  let total = $state('');
  /** One amount per document, keyed by the document's id. Absent means "not allocated". */
  let amounts = $state<Record<string, string>>({});
  let methods = $state<PaymentMethod[] | null>(null);
  let methodsFailed = $state(false);
  let submitting = $state(false);
  let failed = $state(false);

  // Only **active** methods are offered: `is_active` is a picker concern by design, and a
  // retired method must not be offered for new money even though history keeps naming it.
  $effect(() => {
    let live = true;
    listPaymentMethods()
      .then((rows) => {
        if (!live) return;
        methods = rows.filter((m) => m.is_active);
      })
      .catch(() => {
        if (live) methodsFailed = true;
      });
    return () => {
      live = false;
    };
  });

  const draft = $derived<CollectionDraft>({
    methodId,
    total,
    lines: unpaid.map((doc) => ({
      debtId: doc.id,
      outstanding: doc.outstanding,
      amount: amounts[doc.id] ?? ''
    }))
  });
  const preview = $derived(allocationPreview(draft));
  const gate = $derived(canSubmitCollection(draft));

  const blocker = $derived<Record<CollectionBlocker, string>>({
    method: lang.t.collect_need_method,
    total: lang.t.collect_need_total,
    overAllocated: lang.t.collect_over_allocated,
    lineOverDebt: lang.t.collect_line_over_debt,
    duplicateLine: lang.t.collect_duplicate_line,
    lineNotPositive: lang.t.collect_line_not_positive,
    lineUnreadable: lang.t.collect_line_unreadable
  });
  const submitLabel = $derived(
    side === 'customer' ? lang.t.collect_submit_customer : lang.t.collect_submit_supplier
  );

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const body = collectionRequest(draft);
    if (body === null) return;
    submitting = true;
    failed = false;
    try {
      const collection =
        side === 'customer'
          ? await collectFromCustomer(accountId, body)
          : await paySupplier(accountId, body);
      onrecorded(collection);
    } catch {
      // A refusal wrote nothing on the server, so the form keeps its figures and the operator
      // adjusts them. That is why this is a message and not a reset.
      failed = true;
    } finally {
      submitting = false;
    }
  }
</script>

<form class="collect" onsubmit={submit} aria-labelledby="collect-h">
  <h4 id="collect-h" class="heading">{lang.t.collect_method}</h4>

  <div class="row">
    <label class="field">
      <span class="label">{lang.t.collect_method}</span>
      <select class="control" bind:value={methodId} disabled={submitting}>
        <option value="">—</option>
        {#each methods ?? [] as m (m.id)}
          <option value={m.id}>{m.name}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      <span class="label">{lang.t.collect_amount}</span>
      <input
        class="control figures"
        inputmode="decimal"
        bind:value={total}
        disabled={submitting}
        placeholder={lang.t.collect_amount_hint}
      />
    </label>
  </div>
  {#if methodsFailed}
    <p class="hint" role="status">{lang.t.sales_method_failed}</p>
  {/if}

  {#if unpaid.length > 0}
    <fieldset class="manual" disabled={submitting}>
      <legend class="label">{lang.t.collect_manual_heading}</legend>
      <ul class="lines">
        {#each unpaid as doc (doc.id)}
          <li class="line">
            <!-- The source document's id, because that is the one a cashier can look up; the
                 obligation is an internal row (the mistake slice 1 found three times). -->
            <span class="doc-id">#{doc.sourceDocumentId.slice(0, 8)}</span>
            <span class="owes">{formatMoney(doc.outstanding)}</span>
            <input
              class="control figures line-amount"
              inputmode="decimal"
              aria-label={`${lang.t.collect_amount} #${doc.sourceDocumentId.slice(0, 8)}`}
              value={amounts[doc.id] ?? ''}
              oninput={(e) => (amounts = { ...amounts, [doc.id]: e.currentTarget.value })}
              placeholder={lang.t.collect_amount_hint}
            />
          </li>
        {/each}
      </ul>
    </fieldset>
  {/if}

  <!-- **The remainder as a figure and a sentence, never as a list of documents** (S9). -->
  <p class="remainder" role="status">
    {#if preview.overAllocated}
      {lang.t.collect_over_allocated}
    {:else if Number(preview.remainder) === 0}
      {lang.t.collect_remainder_none}
    {:else}
      {lang.t.collect_remainder(formatMoney(preview.remainder))}
    {/if}
  </p>

  {#if failed}
    <p class="state error" role="alert">{lang.t.collect_failed}</p>
  {/if}

  <!-- The primary action: a solid ink bar at the foot of the form it closes, with the reason
       stated when it cannot fire yet (`DESIGN.md`, and the pattern `Catalog.svelte` sets). -->
  <div class="foot">
    {#if !gate.ok}
      <p class="hint blocker">{blocker[gate.reason]}</p>
    {/if}
    <div class="buttons">
      <button type="button" class="ghost" onclick={oncancel} disabled={submitting}>
        {lang.t.collect_cancel}
      </button>
      <button type="submit" class="do" disabled={!gate.ok || submitting}>{submitLabel}</button>
    </div>
  </div>
</form>

<style>
  .collect {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--rule);
  }
  .heading {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink-soft);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .label {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  /* Fields: white surface, 1px strong-rule border, 2px radius, figure fields tabular and
     right-aligned (`DESIGN.md`, Fields). */
  .control {
    background: var(--surface);
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
    padding: 8px 9px;
    font: inherit;
    font-size: var(--step-1);
    color: var(--ink);
  }
  .control:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .figures {
    font-family: var(--font-figure);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .manual {
    border: 0;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lines {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .line {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) 92px;
    align-items: center;
    gap: 8px;
  }
  .doc-id {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  .owes {
    font-family: var(--font-figure);
    font-variant-numeric: tabular-nums;
    font-size: var(--step-0);
    color: var(--ink-soft);
    text-align: right;
  }
  .remainder {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  .foot {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 2px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .ghost {
    background: none;
    border: 1px dashed var(--rule-strong);
    color: var(--ink);
    border-radius: 2px;
    padding: 10px 12px;
    font-size: var(--step-0);
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .do {
    flex: 1;
    background: var(--ink);
    color: var(--surface);
    border: 1px solid var(--ink);
    border-radius: 2px;
    padding: 10px 14px;
    font-family: var(--font-figure);
    font-size: var(--step-1);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    cursor: pointer;
  }
  .do:hover:not(:disabled) {
    background: color-mix(in srgb, var(--ink) 88%, black);
  }
  .do:disabled {
    background: var(--surface);
    color: var(--ink-soft);
    border: 1px dashed var(--rule-strong);
    cursor: default;
  }
  .hint {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  .blocker {
    color: var(--state-pending);
  }
  .state.error {
    font-size: var(--step--1);
    color: var(--state-void);
  }
</style>
