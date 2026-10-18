import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
// Vite 8's own `defineConfig` no longer accepts a `test` key (it is not part of its
// `UserConfigExport`), so the type comes from vitest, whose config type extends Vite's
// with the `test` block this project uses.
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit({ preprocess: vitePreprocess(), adapter: adapter() })],
  // `PUBLIC_API_BASE` is SvelteKit's naming convention, not Vite's: Vite only
  // exposes variables whose prefix it was told about, and `@sveltejs/kit` 3.1.0
  // does not set one here. So `import.meta.env.PUBLIC_API_BASE` compiled to
  // `undefined` on every build and `api.ts` always fell back to `:3001` —
  // measured on the emitted bundle, which contained only `localhost:3001` and no
  // trace of the value the build was given.
  //
  // It went unnoticed because in CI the fallback happens to be right (the backend
  // really is on 3001), which made the variable decorative. Locally it points at
  // whatever container holds that port, so pointing the e2e at a freshly built
  // backend silently talked to a stale one and surfaced as a wall of undefined
  // fields rather than as a connection error.
  envPrefix: ['VITE_', 'PUBLIC_'],
  test: {
    include: ['tests/unit/**/*.{test,spec}.ts'],
    environment: 'jsdom'
  }
});
