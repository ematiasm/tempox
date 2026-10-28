<script lang="ts">
  import { lang } from '#lib/i18n/index.svelte.js';
  import PurchaseDraft from '#lib/components/PurchaseDraft.svelte';
  import PurchaseLedger from '#lib/components/PurchaseLedger.svelte';

  // Bumping the token tells the ledger a purchase was just recorded, so the day
  // reloads and the new document appears.
  let refreshToken = $state(0);

  // The sheet exposes its own `triggerRecord()`, so the pinned action, F2 and
  // Ctrl+Enter are one code path rather than three that can drift.
  let draft = $state<PurchaseDraft | undefined>(undefined);
  let ready = $state(false);
  // The period's document count, reported by the ledger once its reads land.
  let docCount = $state<number | null>(null);

  /**
   * F2 continues the purchase, and what continuing means depends on state.
   *
   * With lines in the sheet it records; with an empty sheet it puts the caret in
   * the product field. One key, one meaning per state — the same rule the sales
   * surface settled, kept identical so the two screens teach the same reflex.
   */
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'F2') {
      e.preventDefault();
      if (ready) draft?.triggerRecord();
      else document.getElementById('product-search')?.focus();
    }
  }

  // The product field is where a purchase starts, so it holds the caret on
  // arrival: the operator can type the first product without touching the mouse.
  $effect(() => {
    queueMicrotask(() => document.getElementById('product-search')?.focus());
  });
</script>

<svelte:window {onkeydown} />

<div class="page">
  <!-- No band. The statement world gives the figures the loudest voice and puts
       the single next action at the foot of the draft it closes; a coloured
       region above everything would shout before the page had said anything.
       This mirrors /sales exactly, so the two screens read as one product. -->
  <header class="head">
    <div class="head-line">
      <h1>{lang.t.purchases_title}</h1>
      <span class="tagline">{lang.t.purchases_tagline}</span>
    </div>
    {#if docCount !== null}
      <div class="count">
        <span class="count-n">{docCount}</span>
        <span class="count-l">
          {docCount === 1 ? lang.t.sales_document_one : lang.t.sales_documents}
        </span>
      </div>
    {/if}
  </header>

  <div class="workspace">
    <section class="panel panel-draft" aria-label={lang.t.purchases_draft_title}>
      <PurchaseDraft
        bind:this={draft}
        onrecorded={() => refreshToken++}
        onready={(r) => (ready = r)}
      />
      <!-- The page's single next action, pinned to the panel's foot and always on
           screen. The mobile stack scrolls, so this sticks to the viewport edge
           rather than floating over the fields above it. -->
      <div class="action-foot">
        {#if !ready}
          <span class="why">{lang.t.purchases_action_hint}</span>
        {/if}
        <button
          class="do"
          disabled={!ready}
          onclick={() => draft?.triggerRecord()}
          aria-keyshortcuts="F2 Control+Enter"
        >
          {lang.t.purchases_action} <kbd>F2</kbd>
        </button>
      </div>
    </section>
    <section class="panel panel-ledger" aria-label={lang.t.purchases_ledger_title}>
      <PurchaseLedger {refreshToken} oncount={(c) => (docCount = c)} />
    </section>
  </div>
</div>

<style>
  :global(.content) {
    height: 100dvh;
    overflow: hidden;
  }

  .page {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .head {
    padding: 22px 26px 16px 28px;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 20px;
  }
  .head-line {
    display: flex;
    align-items: baseline;
    gap: 14px;
    flex-wrap: wrap;
  }
  .head h1 {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 600;
    line-height: 1.1;
    letter-spacing: -0.01em;
    color: var(--ink);
  }
  .tagline {
    font-size: var(--step-0);
    color: var(--ink-soft);
  }
  .count {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .count-n {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    line-height: 1;
    color: var(--ink);
  }
  .count-l {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink-soft);
  }

  .workspace {
    display: grid;
    grid-template-columns: minmax(520px, 1fr) minmax(420px, 640px);
    gap: var(--panel-gap);
    align-items: stretch;
    padding: 0 26px 26px 28px;
    min-height: 0;
  }
  .workspace > section {
    min-height: 0;
  }

  .panel-draft {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .action-foot {
    flex-shrink: 0;
    /* Pushed to the panel's foot. Without this it follows the sheet's content and
       stops wherever that ends — measured on an empty draft, the action landed at
       y≈395 inside a panel running to y≈874, leaving ~480px of surface below an
       action that read as the page having run out of content. The panel is a
       column that fills its grid track; its action belongs at the bottom of that
       track, not at the bottom of the content. */
    margin-top: auto;
    padding: 16px 20px 20px;
    border-top: 1px solid var(--rule);
    background: var(--surface);
  }
  .action-foot .why {
    display: block;
    margin: 0 0 8px;
    font-size: var(--step--1);
    color: var(--ink-soft);
    text-align: center;
  }

  .panel-ledger {
    min-height: 480px;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  /* Solid ink, never the accent: the accent belongs to the running balance on
     /sales and to the live destination, and this page does not get a second
     claim on it. */
  .do {
    width: 100%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: var(--ink);
    color: var(--surface);
    border: 1px solid var(--ink);
    border-radius: var(--radius);
    padding: 12px 18px;
    font-family: var(--font-figure);
    font-size: var(--step-2);
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
  .do kbd {
    font-family: var(--font-text);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: none;
    color: inherit;
    border: 1px solid currentColor;
    border-radius: 2px;
    padding: 1px 5px;
    opacity: 0.75;
  }

  @media (max-width: 1080px) {
    :global(.content) {
      height: auto;
      min-height: 100dvh;
      overflow: visible;
    }
    .workspace {
      grid-template-columns: minmax(0, 1fr);
    }
    .panel-draft {
      max-height: none;
    }
    .action-foot {
      position: sticky;
      bottom: 0;
      z-index: 3;
    }
  }
</style>
