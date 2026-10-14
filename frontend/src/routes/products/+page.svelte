<script lang="ts">
  import { lang } from '$lib/i18n/index.svelte';
  // T3 scope: the shell this route lives in. The catalog, its search and the
  // create form arrive in T7.
</script>

<header class="band">
  <div>
    <h1>{lang.t.products_title}</h1>
    <p>{lang.t.products_tagline}</p>
  </div>
  <button class="action" disabled>{lang.t.products_action} <kbd>F2</kbd></button>
</header>

<section class="body" aria-labelledby="catalog-h">
  <div class="head">
    <div>
      <h2 id="catalog-h">{lang.t.products_catalog_title}</h2>
      <div class="sub">{lang.t.products_catalog_sub}</div>
    </div>
  </div>
  <p class="pending">{lang.t.products_catalog_pending}</p>
</section>

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
  .action:disabled {
    opacity: 0.55;
    cursor: default;
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

  .body {
    margin: 22px 26px 0 28px;
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-radius: 4px;
  }
  .head {
    padding: 14px 18px 12px;
    border-bottom: 1px solid var(--rule);
  }
  .head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-3);
    font-weight: 600;
  }
  .sub {
    color: var(--ink-soft);
    font-size: var(--step-0);
    margin-top: 2px;
  }
  .pending {
    padding: 26px 18px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
</style>
