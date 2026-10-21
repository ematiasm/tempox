<script lang="ts">
  import { lang } from '#lib/i18n/index.svelte.js';
  import Catalog from '#lib/components/Catalog.svelte';

  // How many products the search matched, reported by the catalog once its read
  // lands. On the other two surfaces this figure is a period; here it is the
  // size of a register, which is why it says "productos" and not "comprobantes".
  let productCount = $state<number | null>(null);

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
    {#if productCount !== null}
      <div class="count">
        <span class="count-n">{productCount}</span>
        <span class="count-l">
          {productCount === 1 ? lang.t.products_product_one : lang.t.products_products}
        </span>
      </div>
    {/if}
  </header>

  <Catalog oncount={(c) => (productCount = c)} />
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
