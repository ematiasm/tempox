<script lang="ts">
  import '../app.css';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { page } from '$app/state';
  import { resolve } from '$app/paths';

  let { children } = $props();

  // The document's language follows the interface's (S14). Without it a screen
  // reader keeps announcing Spanish text with an English voice, and the browser
  // keeps offering to translate a page that is already in the shop's language.
  // `app.html` pins the pre-hydration value; this keeps it true afterwards.
  $effect(() => {
    document.documentElement.lang = lang.current;
  });

  // The rail carries the three jobs the shop does: sell, buy, and look after
  // the catalog. `/notes` stays reachable by URL as the walking-skeleton proof,
  // but it is not a destination: the shop does not sell notes, and a nav that
  // lists everything it can reach is a map of the repo rather than of the job.
  // The route ids, not the resolved paths: `resolve()` runs at the three places
  // that actually render or compare a URL, so a configured `paths.base` would
  // reach the rail without the comparison and the href ever disagreeing.
  // `as const` keeps them as literals, which is what `resolve()` requires: it is
  // typed to the app's own route ids, not to any string.
  const tabs = $derived([
    { href: '/sales', label: lang.t.nav_sales },
    { href: '/purchases', label: lang.t.nav_purchases },
    { href: '/products', label: lang.t.nav_products }
  ] as const);
</script>

<div class="app">
  <nav class="rail" aria-label={lang.t.nav_main}>
    <a class="brand" href={resolve('/sales')}>Tempox</a>
    <ul class="tabs">
      {#each tabs as tab (tab.href)}
        <li>
          <a
            class="tab"
            class:is-active={page.url.pathname.startsWith(resolve(tab.href))}
            href={resolve(tab.href)}
            aria-current={page.url.pathname.startsWith(resolve(tab.href)) ? 'page' : undefined}
          >
            {tab.label}
          </a>
        </li>
      {/each}
    </ul>
    <div class="rail-foot">
      <button class="lang" onclick={() => lang.toggle()} aria-label={lang.t.lang_toggle}>
        {lang.other}
      </button>
      <span class="version">v2</span>
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
       so the rail grows into a tall block instead of a bar once it becomes
       horizontal. The rail is chrome: it should be exactly as tall as it is. */
    align-content: start;
  }

  /* The rail is the page's margin, not a coloured slab. In the statement world
     the ground is the same on both sides and the only separation is a hairline:
     a filled block would make the navigation louder than the figures, which is
     the opposite of what this world weights. */
  .rail {
    background: transparent;
    border-right: 1px solid var(--rule);
    color: var(--ink-soft);
    display: flex;
    flex-direction: column;
    z-index: 2;
    position: relative;
  }

  .brand {
    font-family: var(--font-figure);
    font-weight: 700;
    font-size: 14px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    padding: 22px 0 24px;
    color: var(--ink);
    text-align: center;
    text-decoration: none;
  }

  .tabs {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  /* The active destination is marked by the accent as a left rule and accent
     text — never a filled block. A block would compete with the day's figures
     for the eye, and the figures are what the shop is looking at. */
  .tab {
    display: block;
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    text-align: center;
    padding: 12px 0;
    color: var(--ink-soft);
    text-decoration: none;
    border-left: 3px solid transparent;
  }

  .tab:hover {
    color: var(--ink);
  }

  .tab.is-active {
    color: var(--accent);
    border-left-color: var(--accent);
  }

  .rail-foot {
    margin-top: auto;
    padding: 16px 0 18px;
    border-top: 1px solid var(--rule);
    text-align: center;
    display: grid;
    gap: 8px;
  }

  .lang {
    background: none;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    color: var(--ink-soft);
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    padding: 5px 0;
    margin: 0 auto;
    width: 58px;
    cursor: pointer;
  }

  .lang:hover {
    border-color: var(--ink-soft);
    color: var(--ink);
  }

  .version {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink-soft);
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
      border-right: 0;
      border-bottom: 1px solid var(--rule);
    }
    .brand {
      padding: 14px 16px 14px 0;
      border-right: 1px solid var(--rule);
    }
    .tabs {
      display: flex;
      flex-direction: row;
      margin: 0 0 0 10px;
      gap: 4px;
    }
    .tab {
      padding: 8px 10px;
      border-left: 0;
      border-bottom: 2px solid transparent;
    }
    .tab.is-active {
      border-left-color: transparent;
      border-bottom-color: var(--accent);
    }
    .rail-foot {
      margin: 0 0 0 auto;
      border-top: 0;
      padding: 0;
      display: flex;
      align-items: center;
      gap: 10px;
      /* The nav bar holds a brand, three destinations and a language toggle.
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
