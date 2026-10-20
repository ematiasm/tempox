<script lang="ts">
  import { lang } from '#lib/i18n/index.svelte.js';
  import SaleDraft from '#lib/components/SaleDraft.svelte';
  import Ledger from '#lib/components/Ledger.svelte';

  // Bumping the token tells the ledger a sale was just recorded, so the day
  // reloads and the new document appears with its stamp.
  let refreshToken = $state(0);

  // The composition puts the page's single next action in the band. The sheet
  // exposes its own `record()`, so the band button and Ctrl+Enter are one code
  // path rather than two that can drift.
  let draft = $state<SaleDraft | undefined>(undefined);
  let ready = $state(false);

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'F2') {
      e.preventDefault();
      // F2 is printed on the band's action, so it records. A key labelled on a
      // control and bound to something else teaches the cashier that the
      // shortcut lies.
      if (ready) draft?.record();
      else document.getElementById('product-search')?.focus();
    }
  }

  // The product field is where a sale starts, so it holds the caret on arrival:
  // the cashier can type the first product without touching the mouse or
  // pressing anything first.
  $effect(() => {
    queueMicrotask(() => document.getElementById('product-search')?.focus());
  });
</script>

<svelte:window {onkeydown} />

<!-- The header carries the day's count, not a coloured band with the action in
     it. In the statement world the page does not shout: the figures are the
     loudest thing on it, and the single action sits where the work happens —
     at the foot of the draft it closes. The action is still reachable from the
     keyboard exactly as before, so F2 keeps meaning one thing. -->
<header class="head">
  <div class="head-line">
    <h1>{lang.t.sales_title}</h1>
    <span class="tagline">{lang.t.sales_tagline}</span>
  </div>
</header>

<div class="workspace">
  <section class="panel" aria-label={lang.t.sales_draft_title}>
    <SaleDraft bind:this={draft} onrecorded={() => refreshToken++} onready={(r) => (ready = r)} />
    <!-- The page's single next action, at the foot of the draft it closes. It
         used to live in a coloured band above everything, which made the page
         shout before it had said anything; here it sits where the work ends.
         F2 still reaches it through the route's handler, so the shortcut keeps
         meaning exactly one thing. -->
    <div class="action-foot">
      <button
        class="do"
        disabled={!ready}
        onclick={() => draft?.record()}
        aria-keyshortcuts="F2 Control+Enter"
      >
        {lang.t.sales_action} <kbd>F2</kbd>
      </button>
    </div>
  </section>
  <section class="panel panel-ledger" aria-label={lang.t.sales_ledger_title}>
    <Ledger {refreshToken} />
  </section>
</div>

<style>
  .head {
    padding: 22px 26px 16px 28px;
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
  /* Beside the heading, never above it. A kicker is a ban in this system's
     craft floor: the heading carries its own weight, and a label over it is
     decoration pretending to be hierarchy. */
  .tagline {
    font-size: var(--step-0);
    color: var(--ink-soft);
  }

  /* Two closed panels, separated by a 24px gap AND their own edges. The border
     is what separates them; the gap only keeps the rules from touching. Each
     area starts and stops on the page, which is what makes a row belonging to
     the day read differently from a row belonging to the draft. */
  .workspace {
    display: grid;
    grid-template-columns: 480px minmax(0, 1fr);
    gap: var(--panel-gap);
    align-items: start;
    padding: 0 26px 26px 28px;
  }

  .panel-ledger {
    /* The day is the tall column: it grows and the draft stays where it is. */
    min-height: 480px;
  }

  /* The action is a solid ink bar, never the accent: the accent on this page
     belongs to the running balance, and a second use of it would dilute the one
     number the column exists to make readable. */
  .action-foot {
    padding: 0 20px 20px;
  }
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
    background: #0a1622;
  }
  .do:disabled {
    background: var(--surface);
    color: var(--ink-soft);
    border-color: var(--rule-strong);
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
    .workspace {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
