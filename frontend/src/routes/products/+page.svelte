<script lang="ts">
  import { lang } from '#lib/i18n/index.svelte.js';
  import Catalog from '#lib/components/Catalog.svelte';

  // What the catalog is showing, reported by the catalog once its read lands.
  //
  // NOT a total. `GET /api/products` returns a plain array with no count, and the
  // catalog pages 50 at a time, so the only figure available here is the page's
  // own size. The first version of this header printed that page size as "50
  // PRODUCTOS" above a shop that sells hundreds — the same defect this branch
  // fixed twice on the money side, where a footer derived from the loaded rows
  // called itself the period. Instead of shipping it a third time, the header
  // reports what it actually knows: how many rows are listed, and whether more
  // exist. A true total needs a `COUNT(*)` from the API.
  let shown = $state<{ count: number; more: boolean } | null>(null);

  /**
   * F2 puts the caret in the create form's first field, and the page's action
   * prints it. The key is wired here rather than in the component so that one
   * file answers "what does F2 do on this page".
   */
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'F2') {
      e.preventDefault();
      document.getElementById('new-name')?.focus();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="page">
  <!-- The header carries the register's size, not a coloured band with the
       action inside it. In the statement world the page does not shout: the
       figures are the loudest thing on it, and the single action sits where the
       work happens — at the foot of the form it closes. This page used to open
       with a full-bleed accent band carrying the action, which is the exact
       shape DESIGN.md's Don't list names, and it was the last one left. -->
  <header class="head">
    <div class="head-line">
      <h1>{lang.t.products_title}</h1>
      <span class="tagline">{lang.t.products_tagline}</span>
    </div>
    <!-- The figure is honest about its scope: `+` when the read hit the page
         size, so nobody reads 50 as "the shop sells 50 products". -->
    {#if shown !== null}
      <div class="count">
        <span class="count-n">{shown.more ? `${shown.count}+` : shown.count}</span>
        <span class="count-l">
          {#if shown.more}
            {lang.t.products_products_shown}
          {:else}
            {shown.count === 1 ? lang.t.products_product_one : lang.t.products_products}
          {/if}
        </span>
      </div>
    {/if}
  </header>

  <Catalog onshown={(s) => (shown = s)} />
</div>

<style>
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
  /* The register's size, set like the other figures: condensed, tabular, quiet.
     A count is a fact the shop reads to know how much it sells, not a label for
     the heading. */
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

  /* The page is a two-row grid: the header takes what it needs and the
     workspace gets the rest. Same shape as the other two surfaces, for the same
     reason: the panels divide a known height instead of growing with their
     content. */
  .page {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  /* The page pins itself to the viewport so the panels divide a known height.
     Desktop only: a stacked layout has to be allowed to scroll, or the form's
     own fields end up clipped inside a panel that cannot grow — the blocker the
     sales finish review measured at 390px. */
  :global(.content) {
    height: 100dvh;
    overflow: hidden;
  }

  @media (max-width: 1080px) {
    :global(.content) {
      height: auto;
      min-height: 100dvh;
      overflow: visible;
    }
  }
</style>
