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
  test: {
    include: ['tests/unit/**/*.{test,spec}.ts'],
    environment: 'jsdom'
  }
});
