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
  // The period's document count, reported by the ledger once its reads land.
  let docCount = $state<number | null>(null);

  /**
   * F2 does two jobs, and both are printed where they happen.
   *
   * With lines in the sheet it records; with an empty sheet it puts the caret
   * in the product field. That is one key with one meaning per state, not two
   * shortcuts competing: the cashier presses F2 to "continue the sale", and
   * what continuing means depends on whether there is a sale yet. The button
   * prints F2 because it is the state where F2 records.
   */
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'F2') {
      e.preventDefault();
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

<div class="page">
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
    <!-- The period's size, beside the heading and never above it: the contract
         asks for it in the first viewport, and a count is a fact the shop reads
         to know how big the period is, not a label for the heading. -->
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
    <section class="panel panel-draft" aria-label={lang.t.sales_draft_title}>
      <SaleDraft bind:this={draft} onrecorded={() => refreshToken++} onready={(r) => (ready = r)} />
      <!-- The page's single next action, at the foot of the draft it closes and
         always on screen. It used to live in a coloured band above everything,
         which made the page shout before it had said anything; then it moved
         here and the panel grew past the viewport, which was worse — an action
         you have to scroll to find is one the cashier will not use. Pinned
         below the scrolling middle, with F2 printed on it. -->
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
    </section>
    <section class="panel panel-ledger" aria-label={lang.t.sales_ledger_title}>
      <Ledger {refreshToken} oncount={(c) => (docCount = c)} />
    </section>
  </div>
</div>

<style>
  .head {
    padding: 22px 26px 16px 28px;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 20px;
  }
  /* The period's size, set like the other figures: condensed, tabular, quiet. */
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
    /* The draft is the page's main work, so it gets the extra width: it is the
       only area that benefits from room (more search results, longer product
       names, a wider total). The day's ledger is a reading column and stops
       being useful once its rows fit, so it takes what it needs and no more —
       growing it instead would push every figure further from the eye that is
       scanning them.

       `minmax(560px, ...)`, not `520px`. The draft's own table has a minimum
       width of 550px — the product name, quantity, price, line total and the
       remove control do not compress further — so a 520px track let the table
       escape its panel by 31px on every viewport from 1350px down to the old
       1080px breakpoint. 560 is that minimum plus the panel's own inset. */
    grid-template-columns: minmax(560px, 1fr) minmax(420px, 640px);
    gap: var(--panel-gap);
    align-items: stretch;
    padding: 0 26px 26px 28px;
    min-height: 0;
  }

  /* Grid items default to `min-height: auto`, which refuses to shrink below
     their content. Without this the panels keep their natural height, the grid
     track keeps theirs, and the action lands below the fold. */
  .workspace > section {
    min-height: 0;
  }

  /* The page pins itself to the viewport so the panels divide a known height
     instead of growing with their content.
     `:global` on `.content` is deliberate: that element belongs to the layout.

     Desktop only, and that is the fix for the mobile blocker the finish review
     found: at 390px the panels stack, so pinning the page to the viewport meant
     the draft's own fields were clipped inside a panel that could not grow, and
     the pinned action sat on top of "Nombre del cliente nuevo". A stacked layout
     has to be allowed to scroll; a side-by-side one has to not. */
  :global(.content) {
    height: 100dvh;
    overflow: hidden;
  }

  /* Two columns stop being viable below this width. The draft needs 560px and
     the ledger 420px, plus the gap and the page's own insets: at 1080px the
     pair still fitted the grid but the draft track fell to its 520px floor and
     the table inside it overflowed by 31px on every width down to 1081. The
     breakpoint moves up to where the two tracks genuinely still fit, so there
     is no band between the two layouts where the draft is broken. */
  @media (max-width: 1120px) {
    /* The stack scrolls like a document. The action is still pinned — but to the
       bottom of the viewport, with the panel's own padding holding it clear of
       whatever it would otherwise cover. */
    :global(.content) {
      height: auto;
      min-height: 100dvh;
      overflow: visible;
    }
    .panel-draft {
      max-height: none;
    }
    .action-foot {
      position: sticky;
      bottom: 0;
      z-index: 3;
    }
    .workspace {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  /* The page is a two-row grid: the header takes what it needs and the
     workspace gets the rest. `minmax(0, 1fr)` rather than `1fr` because a grid
     track's default minimum is its content, which is what let the panels push
     past the viewport in the first place. */
  .page {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  /* The draft panel is a column that scrolls its middle rather than one that
     grows past the viewport: the action must stay on screen, because an action
     you have to scroll to find is an action the cashier will not use. */
  .panel-draft {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .action-foot {
    /* Pinned to the panel's foot, outside the scrolling middle, with the
       breathing room a primary action needs. It sat flush against the panel's
       edges and against the sheet's own border, which read as a caption rather
       than a control. */
    flex-shrink: 0;
    padding: 16px 20px 20px;
    border-top: 1px solid var(--rule);
    background: var(--surface);
  }

  .action-foot .why {
    /* When there is nothing to record the button says why, instead of sitting
       greyed out with no explanation. A disabled control that looks like text
       is a control the cashier never learns exists — which is exactly what
       happened: the action was there and unreachable to the eye. */
    display: block;
    margin: 0 0 8px;
    font-size: var(--step--1);
    color: var(--ink-soft);
    text-align: center;
  }

  .panel-ledger {
    /* The day is a reading column: it fills its share of the height and
       scrolls its own rows, never the page. */
    min-height: 480px;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  /* The action is a solid ink bar, never the accent: the accent on this page
     belongs to the running balance, and a second use of it would dilute the one
     number the column exists to make readable. */
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
    /* Outlined rather than transparent: a disabled primary action must still
       read as the button it will become. When it was a white box with grey text
       it read as a caption, and the cashier could not tell the action existed
       at all. The hint above it now says why it is not ready yet. */
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
</style>
