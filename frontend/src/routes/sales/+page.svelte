<script lang="ts">
  import { lang } from '$lib/i18n/index.svelte';
  import SaleDraft from '$lib/components/SaleDraft.svelte';
  import Ledger from '$lib/components/Ledger.svelte';

  // Bumping the token tells the ledger a sale was just recorded, so the day
  // reloads and the new document appears with its stamp.
  let refreshToken = $state(0);
</script>

<header class="band">
  <div>
    <h1>{lang.t.sales_title}</h1>
    <p>{lang.t.sales_tagline}</p>
  </div>
</header>

<div class="workspace">
  <SaleDraft onrecorded={() => refreshToken++} />
  <Ledger {refreshToken} />
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
  }
</style>
