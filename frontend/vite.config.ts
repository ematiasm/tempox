import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
// Vite 8's own `defineConfig` no longer accepts a `test` key (it is not part of its
// `UserConfigExport`), so the type comes from vitest, whose config type extends Vite's
// with the `test` block this project uses.
import { defineConfig } from 'vitest/config';

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
      include: ['tests/unit/**/*.{test,spec}.ts'],
      environment: 'jsdom'
    }
  };
});
