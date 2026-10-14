<script lang="ts">
  import { lang } from '$lib/i18n/index.svelte';
  import Catalog from '$lib/components/Catalog.svelte';
</script>

<header class="band">
  <div>
    <h1>{lang.t.products_title}</h1>
    <p>{lang.t.products_tagline}</p>
  </div>
  <!-- The page's single next action lives in the band, as on /sales. It moves
       the caret to the first field of the create form rather than opening a
       modal: creating a product is a few fields, not an interruption. -->
  <button class="action" onclick={() => document.getElementById('new-name')?.focus()}>
    {lang.t.products_action} <kbd>F2</kbd>
  </button>
</header>

<Catalog />

<style>
  .band {
    background: var(--crimson);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 26px 14px 28px;
    min-height: 78px;
  }
  .band h1 {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 600;
    line-height: 1.1;
  }
  .band p {
    color: rgb(255 255 255 / 0.94);
    font-size: var(--step-0);
    margin-top: 2px;
  }
  .action {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    background: var(--paper-raised);
    color: var(--ink);
    border: 0;
    border-radius: var(--radius);
    padding: 12px 18px;
    font-family: var(--font-figure);
    font-size: var(--step-3);
    font-weight: 600;
    box-shadow:
      0 2px 0 rgb(0 0 0 / 0.18),
      0 8px 18px -10px rgb(0 0 0 / 0.55);
    cursor: pointer;
  }
  .action kbd {
    font-family: var(--font-text);
    font-size: var(--step--1);
    font-weight: 600;
    color: var(--ink-soft);
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
    padding: 2px 5px;
    background: var(--paper);
  }
</style>
