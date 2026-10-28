<script lang="ts">
  import { lang } from '#lib/i18n/index.svelte.js';
  import PurchaseDraft from '#lib/components/PurchaseDraft.svelte';
  import PurchaseLedger from '#lib/components/PurchaseLedger.svelte';

  // Bumping the token tells the ledger a purchase was just recorded, so the day
  // reloads and the new document appears in the stack.
  let refreshToken = $state(0);
  let draft = $state<PurchaseDraft | undefined>(undefined);
  let ready = $state(false);
  let docCount = $state<number | null>(null);
</script>

<!-- The pad and the day, side by side — the same composition as `/sales`, because a
     purchase is the same document in the other direction. The slip being written
     is the left column, the day's stack is the right one, and the rule between
     them is the divider rather than air.

     No band: the statement world gives the figures the loudest voice and puts the
     single next action at the foot of the slip it closes. -->
<div class="page">
  <section class="pad" aria-labelledby="pad-h">
    <header class="pad-head">
      <div class="head-line">
        <h1 id="pad-h">{lang.t.purchases_title}</h1>
        <span class="tagline">{lang.t.purchases_tagline}</span>
      </div>
      {#if docCount !== null}
        <div class="count">
          <span class="count-n figures">{docCount}</span>
          <span class="count-l">
            {docCount === 1 ? lang.t.sales_document_one : lang.t.sales_documents}
          </span>
        </div>
      {/if}
    </header>

    <div class="pad-body">
      <PurchaseDraft
        bind:this={draft}
        onrecorded={() => refreshToken++}
        onready={(r) => (ready = r)}
      />
      <!-- The page's single next action, at the foot of the slip it closes and
           always on screen. The mobile stack scrolls, so this sticks to the
           viewport edge rather than floating over the fields above it. -->
      <div class="action-foot">
        {#if !ready}
          <span class="why">{lang.t.purchases_action_hint}</span>
        {/if}
        <button
          class="do"
          disabled={!ready}
          onclick={() => draft?.triggerRecord()}
          aria-keyshortcuts="Control+Enter"
        >
          {lang.t.purchases_action}
        </button>
      </div>
    </div>
  </section>

  <section class="day" aria-labelledby="day-h">
    <span class="sr-only" id="day-h">{lang.t.purchases_ledger_title}</span>
    <PurchaseLedger {refreshToken} oncount={(c) => (docCount = c)} />
  </section>
</div>

<style>
  /* Two columns divided by one rule: the slip is the work in hand and gets a
     fixed, comfortable width; the day takes the rest, because a stack of
     documents reads better wide than a form does. */
  .page {
    display: grid;
    grid-template-columns: minmax(0, 620px) minmax(360px, 1fr);
    height: 100%;
    min-height: 0;
  }

  .pad {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
    border-right: 1px solid var(--rule);
    padding: 20px 0 20px 26px;
  }

  .pad-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 18px;
    padding: 0 22px 13px 0;
    flex-wrap: wrap;
  }
  .head-line {
    display: flex;
    align-items: baseline;
    gap: 13px;
    flex-wrap: wrap;
  }
  .pad-head h1 {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 600;
    line-height: 1.1;
    letter-spacing: -0.01em;
    color: var(--ink);
    margin: 0;
  }
  .tagline {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }

  .count {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .count-n {
    font-size: var(--step-4);
    font-weight: 700;
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

  /* The slip is a closed surface so the action can sit at its foot and still read
     as part of the document it closes. */
  .pad-body {
    display: flex;
    flex-direction: column;
    min-height: 0;
    margin-right: 22px;
    background: var(--surface);
    border: 1px solid var(--rule);
    border-radius: var(--panel-radius);
    overflow: hidden;
  }

  .action-foot {
    flex-shrink: 0;
    padding: 12px 18px 16px;
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

  /* Solid ink bar, never the accent: the accent belongs to the one figure the
     screen exists to make readable. */
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

  .day {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    padding: 20px 26px 20px 22px;
  }

  .sr-only {
    position: fixed;
    top: 0;
    left: 0;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  :global(.content) {
    height: 100dvh;
    overflow: hidden;
  }

  /* Below this width the two columns stop fitting beside each other: the stack
     scrolls like a document and the action sticks to the bottom edge. */
  @media (max-width: 1120px) {
    :global(.content) {
      height: auto;
      min-height: 100dvh;
      overflow: visible;
    }
    .page {
      grid-template-columns: minmax(0, 1fr);
      height: auto;
    }
    .pad {
      border-right: 0;
      border-bottom: 1px solid var(--rule);
      padding: 18px 20px;
    }
    .pad-head {
      padding: 0 0 12px;
    }
    .pad-body {
      margin-right: 0;
    }
    .action-foot {
      position: sticky;
      bottom: 0;
      z-index: 3;
    }
    .day {
      padding: 18px 20px;
    }
  }
</style>
