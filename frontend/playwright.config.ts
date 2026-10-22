import { defineConfig } from '@playwright/test';

/**
 * The backend's port for the e2e run.
 *
 * Fixed, not ephemeral, and that is a deliberate difference from the backend
 * integration suite. That level owns one process per test and can bind port 0
 * because the test reads the bound address from the binary's own log. This level
 * owns ONE long-lived backend and one long-lived frontend, so the port has to be
 * agreed before either starts.
 *
 * It is also the value the preview is handed as `BACKEND_URL`. The app's `/api`
 * proxy reads that per request, so the origin is a runtime fact rather than
 * something compiled into the bundle — see `src/lib/apiProxy.ts`.
 */
const BACKEND_PORT = process.env.TEST_BACKEND_PORT ?? '3099';
const BACKEND_URL = `http://localhost:${BACKEND_PORT}`;

/**
 * The development backend's port. Nothing in a test run may point here.
 *
 * It is refused by name rather than by convention: `TEST_BACKEND_PORT=3001` would
 * otherwise make every guard above agree with each other and with the wrong
 * database, since the e2e would start its own backend ON the development port and
 * the build would be told that is where the API lives.
 */
const DEVELOPMENT_PORT = '3001';

if (BACKEND_PORT === DEVELOPMENT_PORT) {
  throw new Error(
    `TEST_BACKEND_PORT is ${DEVELOPMENT_PORT}, which is the development backend's port. ` +
      'The e2e suite must never write to the development database: its ledger is ' +
      'append-only by trigger, so a row a test wrote there cannot be deleted on its own. ' +
      'Pick another port, or unset TEST_BACKEND_PORT to use the default 3099.'
  );
}

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
 * Point 2 is why the preview entry below carries `BACKEND_URL` in its own `env`:
 * that entry owns the process the proxy runs in, so it must own the pointer the
 * proxy reads. A `npx playwright test` run by hand therefore still cannot reach
 * the development backend through this file.
 *
 * The retired `PUBLIC_API_BASE` needed the opposite arrangement — it had to be
 * exported by `make test-e2e`, because `webServer.env` never reached the build.
 * Reading the origin at runtime removed that constraint along with the variable.
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
      // `BACKEND_URL` is the preview server's runtime pointer at the backend the
      // suite started above. It replaces `PUBLIC_API_BASE`, which had to be known
      // before the build; the proxy reads this per request instead, so nothing
      // about the deployment is baked into the bundle.
      //
      // `env` is merged as `{ ...process.env, ...webServer.env }`, so the value set
      // here wins over whatever the shell or a `.env` happens to carry. That
      // matters: `.env` names the development backend on `:3001`, and inheriting it
      // would point the suite's proxy at the development database — the desync F6
      // exists to prevent, reintroduced through the environment.
      command: 'npm run build && npm run preview',
      env: {
        BACKEND_URL: BACKEND_URL
      },
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
