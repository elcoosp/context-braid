import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  timeout: 30_000,
  retries: 0,
  use: {
    headless: false, // Requis par Chromium pour charger les extensions MV3
  },
});
