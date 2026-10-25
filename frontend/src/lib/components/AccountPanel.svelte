<script lang="ts">
  /**
   * The side panel that opens when a party row is clicked: three boxes, one snapshot.
   *
   * `v5-customer-account.md` S2, S3, S8. One component for both sides — the direction
   * is data, not markup, and duplicating it is how the two ledgers became 2700 lines.
   *
   * **Pure presentation of what it is given.** The account is fetched by the page, so
   * this component has no loading state of its own to get wrong and can be reasoned
   * about (and reviewed) without a network.
   */
  import { lang } from '#lib/i18n/index.svelte.js';
  import { accountBoxes, type AccountLike } from '#lib/accountPanels.js';
  import { formatMoney } from '#lib/money.js';
  import CollectionForm from './CollectionForm.svelte';
  import type { Collection } from '#lib/api.js';

  let {
    side,
    account,
    notFound,
    error,
    onclose,
    oncollected,
    collected
  }: {
    /** Which box labels to use. The figures mean opposite things on the two sides. */
    side: 'customer' | 'supplier';
    account: AccountLike | null;
    notFound: boolean;
    error: string | null;
    onclose: () => void;
    /**
     * A collection was recorded. The page owns the account state, so it replaces it with
     * the one the response carries — **one snapshot, no second request** (`S6`).
     */
    oncollected: (collection: Collection) => void;
    /**
     * The last collection's result, or `null`. **Rendered, not discarded** (`R3-002` of
     * review `review-b6bebae2ab91c89f`): `applied` with its per-row `source` is the only
     * statement of what the rule decided, and a panel that shows only the new balance leaves
     * the cashier unable to see which documents closed or why.
     */
    collected: Collection | null;
  } = $props();

  /**
   * Whether the form is open. Component state, deliberately: `S2` of slice 1 says only the
   * id lives in the URL, and an open form is not a link anyone sends.
   */
  let collecting = $state(false);

  const collectLabel = $derived(
    side === 'customer' ? lang.t.collect_action_customer : lang.t.collect_action_supplier
  );

  const boxes = $derived(account === null ? null : accountBoxes(account));
  const owedLabel = $derived(
    side === 'customer' ? lang.t.party_panel_owed : lang.t.party_panel_owed_to_them
  );
  const favourLabel = $derived(
    side === 'customer' ? lang.t.party_panel_in_favour : lang.t.party_panel_in_our_favour
  );
  const documentLabel = $derived(
    side === 'customer' ? lang.t.party_panel_unpaid_sales : lang.t.party_panel_unpaid_purchases
  );
</script>

<!-- A `dialog` would trap focus behind a modal backdrop; this is a working panel that
     sits beside the list, so it is a labelled `aside` with an explicit close. -->
<aside class="sheet" aria-labelledby="account-panel-h">
  <header class="sheet-head">
    <!-- The heading must not promise content it does not have: a not-found panel whose
         title says "reading the account…" is a spinner that never resolves (`R3-002`). -->
    <h2 id="account-panel-h">
      {boxes?.name ?? (notFound ? lang.t.party_panel_not_found_short : lang.t.party_panel_loading)}
    </h2>
    <button class="close" type="button" onclick={onclose} aria-label={lang.t.party_panel_close}>
      {lang.t.party_panel_close}
    </button>
  </header>

  {#if error !== null}
    <!-- A failed read is not an empty account and not a missing one. -->
    <p class="state error" role="alert">{error}</p>
  {:else if notFound}
    <p class="state">{lang.t.party_panel_not_found}</p>
  {:else if boxes === null}
    <p class="state">{lang.t.party_panel_loading}</p>
  {:else}
    {#if collected !== null}
      <section class="receipt" aria-labelledby="receipt-h">
        <h3 id="receipt-h">{lang.t.collect_recorded}</h3>
        <ul class="applied">
          {#each collected.applied as row (row.debt_id)}
            <li class="applied-row">
              <!-- **The document's id, not the debt's**: a cashier looks up a sale or a
                   purchase, and an obligation is an internal row with no screen. The same
                   distinction slice 1 found three times, on the surface where it lands on a
                   person reading a receipt. -->
              <span class="doc-id">#{row.source_document_id.slice(0, 8)}</span>
              <span class="applied-amount">{formatMoney(row.amount)}</span>
              <!-- **The `source` is the rule made visible**: the cashier can tell their own
                   decision from the rule's consequence, which is the question they have. -->
              <span class="source" class:manual={row.source === 'manual'}>
                {row.source === 'manual'
                  ? lang.t.collect_applied_manual(formatMoney(row.amount))
                  : lang.t.collect_applied_oldest(formatMoney(row.amount))}
              </span>
            </li>
          {/each}
        </ul>
        {#if Number(collected.unapplied) !== 0}
          <p class="leftover">{lang.t.collect_unapplied(formatMoney(collected.unapplied))}</p>
        {/if}
      </section>
    {/if}
    <div class="boxes">
      <!-- Box 1: identity, first, because it answers "did I click the right row?"
           before any number is read. -->
      <section class="box" aria-labelledby="box-identity">
        <h3 id="box-identity">{lang.t.party_panel_identity}</h3>
        <p class="who">{boxes.name}</p>
      </section>

      <!-- Box 2: the balance. **Both figures, never netted** (S3, L4). -->
      <section class="box" aria-labelledby="box-balance">
        <h3 id="box-balance">{lang.t.party_panel_balance}</h3>
        <dl class="figures">
          <div class="figure">
            <dt>{owedLabel}</dt>
            <dd class="amount">{formatMoney(boxes.owed)}</dd>
          </div>
          <!-- The second figure appears whenever it is non-zero, and also when the
               first is zero: a party who owes nothing but holds money is exactly the
               case a single netted number would erase. -->
          {#if boxes.showsBothFigures || Number(boxes.inFavour) !== 0}
            <div class="figure">
              <dt>{favourLabel}</dt>
              <dd class="amount">{formatMoney(boxes.inFavour)}</dd>
            </div>
          {/if}
        </dl>

        <!-- **The action lives in the balance box**, where the owner put it (L2 of
             `v5-customer-account.md`: "el segundo el saldo y ahi el boton para hacer el pago"). -->
        {#if account !== null && !collecting}
          <button type="button" class="collect-open" onclick={() => (collecting = true)}>
            {collectLabel}
          </button>
        {/if}
        {#if account !== null && collecting}
          <CollectionForm
            {side}
            accountId={account.id}
            unpaid={account.unpaid}
            onrecorded={(collection) => {
              collecting = false;
              oncollected(collection);
            }}
            oncancel={() => (collecting = false)}
          />
        {/if}
      </section>

      <!-- Box 3: the documents behind the number above it. A settled document is
           absent, and the absence is the state — there is no `paid` column. -->
      <section class="box" aria-labelledby="box-documents">
        <h3 id="box-documents">
          {lang.t.party_panel_documents}
          <span class="count">{boxes.unpaidCount}</span>
        </h3>
        {#if boxes.unpaidCount === 0}
          <p class="state">{lang.t.party_panel_none_unpaid}</p>
        {:else if account !== null}
          <ul class="docs">
            {#each account.unpaid as doc (doc.id)}
              <li class="doc">
                <!-- **The source document's id, not the obligation's.** They are
                     different rows (the T3 e2e found the server had them equal), and
                     the one a cashier can look up is the sale or purchase they
                     recorded — an obligation is an internal row with no screen. -->
                <span class="figures-id">#{doc.sourceDocumentId.slice(0, 8)}</span>
                <span class="doc-figures">
                  <span class="outstanding">{formatMoney(doc.outstanding)}</span>
                  <span class="of">/ {formatMoney(doc.total)}</span>
                </span>
              </li>
            {/each}
          </ul>
          <p class="hint">{documentLabel}</p>
        {/if}
      </section>
    </div>
  {/if}
</aside>

<style>
  /* A side sheet, following `PurchaseDraft.svelte`'s `.sheet` precedent rather than
     inventing a fourth panel idiom (S8). 8px radius, white surface, fine border —
     DESIGN.md's Work Panels rule. */
  .sheet {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(420px, 92vw);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-left: 1px solid var(--rule-strong);
    box-shadow: -8px 0 24px rgb(18 33 46 / 8%);
    overflow: auto;
    z-index: 20;
  }
  .sheet-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 18px 22px 14px;
    border-bottom: 1px solid var(--rule);
  }
  .sheet-head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-2);
    font-weight: 600;
    color: var(--ink);
  }
  .close {
    background: none;
    border: 0;
    font: inherit;
    font-size: var(--step--1);
    color: var(--ink-soft);
    cursor: pointer;
    padding: 4px 6px;
  }
  .close:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .boxes {
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .box {
    padding: 16px 22px;
    border-bottom: 1px solid var(--rule);
  }
  .box h3 {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink-soft);
    margin-bottom: 10px;
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .who {
    font-size: var(--step-2);
    color: var(--ink);
  }
  .figures {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .figure {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
  }
  .figure dt {
    font-size: var(--step-0);
    color: var(--ink-soft);
  }
  /* The balance owns the accent rule (DESIGN.md): the figures are the loudest thing
     in the panel, and the debt is the one that is read first. */
  .amount {
    font-family: var(--font-figure);
    font-size: var(--step-3);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--ink);
  }
  .figure:first-child .amount {
    color: var(--accent);
  }
  /* A ghost control, not the primary: the panel's loudest element is the balance figure
     above it, and `DESIGN.md` keeps the accent there (`S10`). */
  .collect-open {
    margin-top: 12px;
    background: none;
    border: 1px dashed var(--rule-strong);
    border-radius: 2px;
    padding: 8px 12px;
    font-size: var(--step-0);
    font-weight: 600;
    color: var(--ink);
    cursor: pointer;
  }
  .collect-open:hover {
    border-color: var(--ink);
  }
  .collect-open:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .receipt {
    padding: 14px 22px;
    border-bottom: 1px solid var(--rule);
    background: var(--surface);
  }
  .receipt h3 {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink-soft);
    margin-bottom: 8px;
  }
  .applied {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .applied-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: var(--step--1);
  }
  .applied-amount {
    font-family: var(--font-figure);
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    color: var(--ink);
  }
  .source {
    color: var(--ink-soft);
  }
  /* The cashier's own decision is the quiet ordinary case; the rule's is stated the same way
     and never colour-alone (`DESIGN.md`'s "Document State Is Readable"). */
  .source.manual {
    color: var(--ink);
  }
  .leftover {
    margin-top: 8px;
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  .count {
    font-family: var(--font-figure);
    font-variant-numeric: tabular-nums;
    color: var(--ink);
  }
  .docs {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .doc {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 0;
    border-top: 1px solid var(--rule);
  }
  .doc:first-child {
    border-top: 0;
  }
  .figures-id,
  .doc-figures {
    font-family: var(--font-figure);
    font-variant-numeric: tabular-nums;
  }
  .figures-id {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  .outstanding {
    font-size: var(--step-1);
    font-weight: 600;
    color: var(--ink);
  }
  .of {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  .state {
    padding: 16px 22px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
  .state.error {
    color: var(--state-void);
  }
  .hint {
    margin-top: 10px;
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
</style>
