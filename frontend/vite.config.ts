import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
// Vite 8's own `defineConfig` no longer accepts a `test` key (it is not part of its
// `UserConfigExport`), so the type comes from vitest, whose config type extends Vite's
// with the `test` block this project uses.
import { defineConfig } from 'vitest/config';
import { playwright } from '@vitest/browser-playwright';

export default defineConfig(() => {
  return {
    plugins: [tailwindcss(), sveltekit({ preprocess: vitePreprocess(), adapter: adapter() })],
    // No `define` and no `envPrefix` for an API origin any more. `api.ts` calls
    // its own origin and the `/api/*` proxy resolves the backend from
    // `BACKEND_URL` at **runtime**, so nothing about a deployment is baked into
    // the bundle. `PUBLIC_API_BASE` and `REQUIRE_PUBLIC_API_BASE` are retired
    // with it; their whole purpose was to get a correct value into a build.
    //
    // The consequence is deliberate (`odd/tasks/docker-prod.md`, S10): a build no
    // longer fails when the origin is unknown, because it does not need to know
    // it. What still holds is the e2e harness, which starts its own backend on
    // `:3099` against `tempox_test` and refuses `:3001` by name.
    test: {
      // `tests/e2e/unique.ts` holds a pure helper the e2e specs depend on, and its
      // uniqueness guarantee is exactly the kind of thing that broke silently before
      // (#52, then again in this very file). It is unit-tested from `tests/unit`,
      // which is why only that directory's specs are collected -- the Playwright
      // files themselves are not vitest's to run.
      //
      // **Components are not unit tests, and pretending otherwise was the blocker.** A
      // `*.svelte.ts` rune can only be exercised by mounting a component, and mounting needs
      // Svelte's *client* build. `svelte`'s package export maps `.` to `index-server.js` by
      // default and to `index-client.js` only under the `browser` condition; SvelteKit owns
      // module resolution, so neither `resolve.conditions` nor `ssr.resolve.conditions` reaches
      // it — measured, both ways. So component tests run in a real browser instead, through the
      // Playwright this project already has. `tests/component` is the project for that; the
      // default project stays fast and headless for pure logic.
      // The provider lives at the root and is a **factory**, not a string: Vitest 5 changed the
      // shape and refuses to start with the old one. Both measured.
      browser: { provider: playwright() },
      projects: [
        {
          extends: true,
          test: {
            name: 'unit',
            include: ['tests/unit/**/*.{test,spec}.ts'],
            environment: 'jsdom'
          }
        },
        {
          extends: true,
          test: {
            name: 'component',
            include: ['tests/component/**/*.{test,spec}.ts'],
            browser: {
              enabled: true,
              instances: [{ browser: 'chromium' as const }],
              headless: true
            }
          }
        }
      ]
    }
  };
});
