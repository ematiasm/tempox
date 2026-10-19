import { defineConfig } from '@playwright/test';

/**
 * The backend's port for the e2e run.
 *
 * Fixed, not ephemeral, and that is a deliberate difference from the backend
 * integration suite. That level owns one process per test and can bind port 0
 * because the test reads the bound address from the binary's own log. This level
 * owns ONE long-lived backend and one long-lived frontend, and the frontend is a
 * compiled artifact: `vite` inlines `PUBLIC_API_BASE` at build time, so the URL has
 * to be known before the build starts, not discovered by it.
 *
 * Measured, not assumed: building with `PUBLIC_API_BASE=http://probe-1234` puts that
 * literal into `_app/immutable/chunks/*.js`. It is not read at runtime.
 */
const BACKEND_PORT = process.env.TEST_BACKEND_PORT ?? '3099';
const BACKEND_URL = `http://localhost:${BACKEND_PORT}`;

/**
 * Playwright starts these in array order and waits for each one, so the backend is
 * serving before the frontend build runs.
 *
 * Two limits worth knowing, both read from the installed Playwright (1.64):
 *
 * 1. `env` is merged as `{ ...process.env, ...webServer.env }`, so an entry adds to
 *    the environment rather than replacing it.
 * 2. That `env` reaches only the process this entry starts. It never reaches the
 *    test process and never reaches the frontend build.
 *
 * Point 2 is why `PUBLIC_API_BASE` is still exported by `make test-e2e` before
 * Playwright is invoked. A `npx playwright test` run by hand would build the
 * frontend with whatever `PUBLIC_API_BASE` the shell has — and with none, `api.ts`
 * falls back to the development backend. Closing that hole is F6's job, and this
 * comment exists so the gap is not mistaken for an oversight.
 */
export default defineConfig({
  webServer: [
    {
      // The branch's own binary, not an image and not a container: this is the only
      // place a real boot is exercised against a real socket at the e2e level.
      command: 'exec ../backend/target/debug/tempox',
      // `url` rather than `port`: the backend answers `/health` only after it has
      // connected to its database and applied migrations, so reaching it proves the
      // boot finished rather than that a process exists.
      url: `${BACKEND_URL}/health`,
      env: {
        // `DATABASE_URL`, not `TEST_DATABASE_URL`: the binary reads the former.
        // `#[sqlx::test]` does too, which cost four review rounds in F4.
        DATABASE_URL: process.env.TEST_DATABASE_URL ?? '',
        BACKEND_PORT,
        RUST_LOG: 'warn'
      },
      // NEVER reuse, in CI or outside it — the opposite of the entry below, on
      // purpose.
      //
      // `reuseExistingServer: true` adopts whatever already answers on :3099: a
      // backend from an earlier run, from a different branch, or bound to another
      // database. The suite would then pass against code this candidate does not
      // contain. `scripts/test-stack.sh`, which F5 replaced, refused to start on an
      // occupied port for exactly this reason; dropping that guard would have been a
      // regression wearing the word "simplification".
      //
      // An occupied :3099 therefore fails hard, with Playwright's own message naming
      // the URL. The preview below may still be reused: it is a static build with no
      // state, and a stale one can only fail loudly, never pass falsely.
      reuseExistingServer: false,
      stdout: 'pipe',
      stderr: 'pipe'
    },
    {
      // `PUBLIC_API_BASE` is inherited from this process's environment, which
      // `make test-e2e` sets. It is consumed by the build in this command.
      command: 'npm run build && npm run preview',
      // `port`, not `url`: the preview answers `/` with a 307 (the app redirects to
      // its first screen), and a `url` check treats a redirect as "not ready" —
      // measured: `curl -o /dev/null -w '%{http_code}' http://localhost:4173/`
      // returns 307, and Playwright then fails with "Process from config.webServer
      // was not able to start". `port` checks that something is listening, which is
      // what this entry actually needs to know. The backend entry keeps `url`,
      // because reaching `/health` there is a real readiness signal.
      port: 4173,
      reuseExistingServer: !process.env.CI
    }
  ],
  testDir: 'tests/e2e',
  use: { baseURL: 'http://localhost:4173' }
});
