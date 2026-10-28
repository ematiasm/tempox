<script lang="ts">
  import { lang } from '#lib/i18n/index.svelte.js';
  import SaleDraft from '#lib/components/SaleDraft.svelte';
  import Ledger from '#lib/components/Ledger.svelte';

  // Bumping the token tells the ledger a sale was just recorded, so the day
  // reloads and the new document appears in the stack.
  let refreshToken = $state(0);

  // The page owns the primary action because it must reach the sheet's `record()`:
  // one code path for the button, the F2 key and Ctrl+Enter, instead of three that
  // can drift. The sheet reports whether it holds something recordable.
  let draft = $state<SaleDraft | undefined>(undefined);
  let ready = $state(false);
  // The period's document count, reported by the ledger once its reads land.
  let docCount = $state<number | null>(null);

  /**
   * F2 does two jobs, and both are printed where they happen.
   *
   * With lines on the slip it records; with an empty slip it puts the caret in
   * the product field. That is one key with one meaning per state, not two
   * shortcuts competing: the cashier presses F2 to "continue the sale", and what
   * continuing means depends on whether there is a sale yet. The button prints F2
   * because it is the state where F2 records.
   */
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'F2') {
      e.preventDefault();
      if (ready) draft?.record();
      else document.getElementById('product-search')?.focus();
    }
  }

  // The product field is where a sale starts, so it holds the caret on arrival:
  // the cashier can type the first product without touching the mouse or pressing
  // anything first.
  $effect(() => {
    queueMicrotask(() => document.getElementById('product-search')?.focus());
  });
</script>

<svelte:window {onkeydown} />

<!-- The pad and the day, side by side.
     The artboard's composition: the slip being written is the left column and the
     day's stack is the right one, divided by a rule rather than by air. The page
     does not shout — the figures are the loudest thing on it — and the single next
     action sits at the foot of the slip it closes. -->
<div class="page">
  <section class="pad" aria-labelledby="pad-h">
    <header class="pad-head">
      <div class="head-line">
        <h1 id="pad-h">{lang.t.sales_title}</h1>
        <span class="tagline">{lang.t.sales_tagline}</span>
      </div>
      <!-- The period's size. The contract asks for it in the first viewport, and a
           count is a fact the shop reads to know how big the period is, not a
           label for the heading. -->
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
      <SaleDraft bind:this={draft} onrecorded={() => refreshToken++} onready={(r) => (ready = r)} />

      <!-- The page's single next action, at the foot of the slip it closes and
           always on screen. Pinned below the scrolling middle, with F2 printed on
           it: an action you have to scroll to find is one the cashier will not
           use. -->
      <div class="action-foot">
        {#if !ready}
          <span class="why">{lang.t.sales_action_hint}</span>
        {/if}
        <button
          class="do"
          disabled={!ready}
          onclick={() => draft?.record()}
          aria-keyshortcuts="F2 Control+Enter"
        >
          {lang.t.sales_action} <kbd>F2</kbd>
        </button>
      </div>
    </div>
  </section>

  <section class="day" aria-labelledby="day-h">
    <span class="sr-only" id="day-h">{lang.t.sales_ledger_title}</span>
    <Ledger {refreshToken} oncount={(c) => (docCount = c)} />
  </section>
</div>

<style>
  /* Two columns, and the rule between them is the divider. The slip is the work
     in hand so it gets a fixed, comfortable width; the day takes the rest,
     because a stack of documents reads better wide than a form does. */
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
  /* Beside the heading, never above it. A kicker is a ban in this system's craft
     floor: the heading carries its own weight, and a label over it is decoration
     pretending to be hierarchy. */
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

  /* The slip is a closed surface, so the action can sit at its foot and still
     read as part of the document it closes. */
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
    /* When there is nothing to record the button says why, instead of sitting
       greyed out with no explanation. A disabled control that looks like text is
       a control the cashier never learns exists. */
    display: block;
    margin: 0 0 8px;
    font-size: var(--step--1);
    color: var(--ink-soft);
    text-align: center;
  }

  /* The action is a solid ink bar, never the accent: the accent on this page
     belongs to the running balance, and a second use of it would dilute the one
     number the stack exists to make readable. */
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
    /* Outlined rather than transparent: a disabled primary action must still read
       as the button it will become. When it was a white box with grey text it
       read as a caption, and the cashier could not tell the action existed. */
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

  .day {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    padding: 20px 26px 20px 22px;
  }

  .sr-only {
    /* `position: fixed` with no offset keeps it 1px wide; an absolutely
       positioned one anchored to no ancestor once measured 538px inside a 390px
       viewport and stretched the whole document. */
    position: fixed;
    top: 0;
    left: 0;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  /* The page pins itself to the viewport so the two columns divide a known height
     instead of growing with their content: the action must stay on screen.
     `:global` on `.content` is deliberate — that element belongs to the layout.

     Desktop only. A stacked layout has to be allowed to scroll: pinning the page
     while the columns stack meant the slip's own fields were clipped inside a box
     that could not grow. */
  :global(.content) {
    height: 100dvh;
    overflow: hidden;
  }

  /* Below this width the two columns stop fitting beside each other. The stack
     then scrolls like a document and the action sticks to the bottom of the
     viewport. */
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
