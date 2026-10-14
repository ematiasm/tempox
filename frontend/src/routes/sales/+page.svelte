<script lang="ts">
  import { lang } from '$lib/i18n/index.svelte';
  import SaleDraft from '$lib/components/SaleDraft.svelte';

  // T5 scope: the draft sheet and the recording write. The ledger, its date
  // scoping and the cash foot arrive in T6 — until then the page shows the
  // composition's second region with an honest account of what it holds.
  let recorded = $state(0);
</script>

<header class="band">
  <div>
    <h1>{lang.t.sales_title}</h1>
    <p>{lang.t.sales_tagline}</p>
  </div>
</header>

<div class="workspace">
  <SaleDraft onrecorded={() => recorded++} />

  <section class="ledger" aria-labelledby="ledger-h">
    <div class="ledger-head">
      <div>
        <h2 id="ledger-h">{lang.t.sales_ledger_title}</h2>
        <div class="sub">{lang.t.sales_ledger_sub}</div>
      </div>
    </div>
    <p class="pending">
      {#if recorded > 0}
        {recorded}
        {recorded === 1 ? lang.t.sales_line_one : lang.t.sales_lines}
        {lang.t.sales_recorded_this_session}
      {:else}
        {lang.t.sales_ledger_pending}
      {/if}
    </p>
  </section>
</div>

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

  .workspace {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 24px;
    padding: 22px 26px 0 28px;
    align-items: start;
  }

  /* Honest placeholders: a region that is not built yet says so, rather than
     showing sample rows a reader could mistake for the shop's real day. */
  .pending {
    padding: 22px 0;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }

  .ledger {
    padding-bottom: 30px;
  }
  .ledger-head {
    padding-bottom: 12px;
  }
  .ledger-head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 600;
    line-height: 1.05;
  }
  .ledger-head .sub {
    color: var(--ink-soft);
    font-size: var(--step-0);
    margin-top: 3px;
  }
</style>
