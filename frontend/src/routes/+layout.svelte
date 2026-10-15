<script lang="ts">
  import '../app.css';
  import { lang } from '$lib/i18n/index.svelte';
  import { page } from '$app/state';

  let { children } = $props();

  // The rail carries Sales and Products only. `/notes` stays reachable by URL
  // as the walking-skeleton proof, but it is not a destination: the shop does
  // not sell notes, and a nav that lists everything it can reach is a map of
  // the repo rather than of the job.
  const tabs = $derived([
    { href: '/sales', label: lang.t.nav_sales },
    { href: '/products', label: lang.t.nav_products }
  ]);
</script>

<div class="app">
  <nav class="rail" aria-label="Main">
    <a class="brand" href="/sales">Tempox</a>
    <ul class="tabs">
      {#each tabs as tab (tab.href)}
        <li>
          <a
            class="tab"
            class:is-active={page.url.pathname.startsWith(tab.href)}
            href={tab.href}
            aria-current={page.url.pathname.startsWith(tab.href) ? 'page' : undefined}
          >
            {tab.label}
          </a>
        </li>
      {/each}
    </ul>
    <div class="rail-foot">
      <button class="lang" onclick={() => lang.toggle()} aria-label="Switch language">
        {lang.t.lang_switch}
      </button>
      <span class="version">v1a</span>
    </div>
  </nav>

  <div class="content">
    {@render children()}
  </div>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: var(--rail-w) minmax(0, 1fr);
    min-height: 100vh;
    /* Without this the grid stretches an auto row to fill the leftover height,
       so the rail grows into a tall navy block instead of a bar once it becomes
       horizontal. The rail is chrome: it should be exactly as tall as it is. */
    align-content: start;
  }

  .rail {
    background: var(--indigo);
    color: #e8e6df;
    display: flex;
    flex-direction: column;
    z-index: 2;
    position: relative;
  }

  .brand {
    font-family: var(--font-figure);
    font-weight: 700;
    font-size: 15px;
    letter-spacing: 0.22em;
    text-transform: uppercase;
    text-align: center;
    padding: 18px 0 22px;
    color: #f2efe6;
    border-bottom: 1px solid rgb(255 255 255 / 0.18);
    text-decoration: none;
  }

  .tabs {
    list-style: none;
    margin: 14px 0 0;
    padding: 0;
    /* Wider than the rail: the active tab overhangs into the work area. Only
       the active one does, which is what makes it read as extended rather than
       as a second column. */
    width: calc(var(--rail-w) + var(--tab-overhang));
  }

  .tab {
    display: block;
    width: var(--rail-w);
    writing-mode: vertical-rl;
    text-orientation: mixed;
    font-family: var(--font-figure);
    font-size: var(--step-2);
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 20px 0 26px;
    margin: 0 auto;
    color: rgb(232 230 223 / 0.8);
    text-decoration: none;
    position: relative;
  }

  .tab:hover {
    color: #fff;
  }

  .tab.is-active {
    background: var(--crimson);
    color: #fff;
    width: calc(var(--rail-w) + var(--tab-overhang));
  }

  .rail-foot {
    margin-top: auto;
    padding-top: 16px;
    border-top: 1px solid rgb(255 255 255 / 0.18);
    text-align: center;
    display: grid;
    gap: 6px;
    padding-bottom: 18px;
  }

  .lang {
    background: none;
    border: 1px solid rgb(255 255 255 / 0.28);
    border-radius: 2px;
    color: inherit;
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    padding: 5px 0;
    margin: 0 auto;
    width: 62px;
    cursor: pointer;
  }

  .lang:hover {
    border-color: rgb(255 255 255 / 0.6);
    color: #fff;
  }

  .version {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgb(232 230 223 / 0.82);
  }

  .content {
    display: flex;
    flex-direction: column;
    min-width: 0;
    position: relative;
    z-index: 1;
  }

  /* The shop's PC is the target. Narrow widths stay usable rather than
     designed-for: the rail becomes a bar rather than disappearing, so
     navigation never hides behind a gesture. */
  @media (max-width: 860px) {
    .app {
      grid-template-columns: minmax(0, 1fr);
    }
    .rail {
      flex-direction: row;
      align-items: center;
      padding: 0 14px;
      align-self: start;
    }
    .brand {
      border-bottom: 0;
      border-right: 1px solid rgb(255 255 255 / 0.18);
      padding: 14px 16px 14px 0;
    }
    .tabs {
      display: flex;
      flex-direction: row;
      width: auto;
      margin: 0 0 0 10px;
      gap: 6px;
    }
    .tab,
    .tab.is-active {
      writing-mode: horizontal-tb;
      width: auto;
      padding: 8px 12px;
    }
    .rail-foot {
      margin: 0 0 0 auto;
      border-top: 0;
      padding: 0;
      display: flex;
      align-items: center;
      gap: 10px;
      /* The nav bar holds a brand, two destinations and a language toggle.
         On a phone the version chip is decoration and the toggle can be its
         own width: without both, the bar is wider than the screen and the
         whole page pans sideways under a thumb. */
      flex-shrink: 0;
    }
    .lang {
      width: auto;
      padding: 5px 9px;
    }
    .version {
      display: none;
    }
  }
</style>
