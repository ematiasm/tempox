import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
// Vite 8's own `defineConfig` no longer accepts a `test` key (it is not part of its
// `UserConfigExport`), so the type comes from vitest, whose config type extends Vite's
// with the `test` block this project uses.
// `loadEnv` comes from `vite`, not from `vitest/config`: that module re-exports
// `defineConfig` but not this helper, and importing it from there fails at config-load
// time with "does not provide an export named 'loadEnv'".
import { loadEnv } from 'vite';
import { defineConfig } from 'vitest/config';

export default defineConfig(({ command }) => {
  // Guard 1: a production build must be TOLD where the backend is.
  //
  // `api.ts` throws at module load when the value is absent, so an unconfigured build
  // is already unusable — but it compiles, and it emits a module that throws only when
  // the page runs. Failing here instead turns that into a build error naming the
  // variable, which is what the person running `vite build` needs to see.
  //
  // Deliberately NOT triggered for the verification-only builds: `make build` and the
  // CI frontend job run `npm run build` to prove the app compiles, and neither will
  // ever serve the result. `vite build` has no flag that distinguishes them, so the
  // check only fires when `PUBLIC_API_BASE` was never intended to be defaulted — that
  // is, when `command === 'build'` and the variable is set to an empty string by a
  // caller that meant to configure it, or when `REQUIRE_PUBLIC_API_BASE` is set (the
  // e2e build does that, in `playwright.config.ts`).
  // Read Vite's own resolution, not only `process.env`.
  //
  // The first version of this read `process.env.PUBLIC_API_BASE` alone, which silently
  // discarded a value supplied through a `.env` file — and `envPrefix` below is exactly
  // what exposes those. Measured: with `.env.local` holding `:4000`, `loadEnv` reports
  // 4000 while `process.env` reports nothing, so a developer who configured their
  // backend that way silently got the hardcoded default (dev) or a throwing client
  // (build). `loadEnv` is Vite's own order of resolution, and `process.env` still wins
  // over it, which matches how Vite itself behaves.
  const fromEnv = loadEnv(command === 'serve' ? 'development' : 'production', process.cwd(), [
    'VITE_',
    'PUBLIC_'
  ]).PUBLIC_API_BASE;

  const configured = process.env.PUBLIC_API_BASE ?? fromEnv;
  const mustBeExplicit = process.env.REQUIRE_PUBLIC_API_BASE === '1';

  if (command === 'build' && mustBeExplicit && !configured) {
    throw new Error(
      'PUBLIC_API_BASE is required for this build and was not set. ' +
        'The e2e build sets it; so does CI. An unset build would compile a client that ' +
        'throws when a page runs, instead of failing here with the reason.'
    );
  }

  return {
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
    // The development default, and it applies to `vite dev` ONLY.
    //
    // `api.ts` no longer falls back to `:3001`, on purpose: that fallback made every
    // unconfigured build point at the development backend, and a test build that does
    // that writes to an append-only ledger. Measured: with `PUBLIC_API_BASE` unset, the
    // literal `localhost:3001` appeared in `_app/immutable/chunks/*.js`.
    //
    // Two legitimate consumers need a default and must not have to set it by hand:
    // `npm run dev` on its own (which sets no environment) and the vitest suite (whose
    // four `api.client` specs call real client functions and build real URLs). Both run
    // through `command === 'serve'`.
    //
    // A BUILD never gets this default. `vite build` without `PUBLIC_API_BASE` compiles an
    // `api.ts` that throws at module load, which is the whole point: the e2e build is
    // started by `make test-e2e` or CI, both of which set the variable, and a bare
    // `npx playwright test` no longer has a path to the development backend. Giving
    // `build` the same default as `serve` would have preserved the exact hole F6 exists
    // to close.
    define: {
      'import.meta.env.PUBLIC_API_BASE': JSON.stringify(
        configured ?? (command === 'serve' ? 'http://localhost:3001' : '')
      )
    },
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
