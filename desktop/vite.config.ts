/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';

// Monaco's package `exports` map hides its ESM tree and CSS; the alias
// reaches the granular entry points so only the diff editor, its features
// and the Monarch grammars are bundled (no language-service workers).
export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  clearScreen: false,
  resolve: {
    alias: {
      'monaco-esm': path.resolve(import.meta.dirname, 'node_modules/monaco-editor/esm/vs'),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  build: {
    target: 'es2022',
    chunkSizeWarningLimit: 6000,
  },
  worker: { format: 'es' },
  test: { include: ['src/**/*.test.ts'], environment: 'node' },
});
