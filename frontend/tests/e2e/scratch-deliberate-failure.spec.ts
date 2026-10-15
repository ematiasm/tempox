import { test, expect } from '@playwright/test';

// SCRATCH ONLY: forces the e2e job to fail so the `if: failure()` artifact
// upload runs. Deleted before this branch is discarded.
test('deliberate failure to exercise the artifact step', async () => {
  expect(1).toBe(2);
});
