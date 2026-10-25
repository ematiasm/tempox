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

  let {
    side,
    account,
    notFound,
    error,
    onclose
  }: {
    /** Which box labels to use. The figures mean opposite things on the two sides. */
    side: 'customer' | 'supplier';
    account: AccountLike | null;
    notFound: boolean;
    error: string | null;
    onclose: () => void;
  } = $props();

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
                <span class="figures-id">#{doc.id.slice(0, 8)}</span>
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
