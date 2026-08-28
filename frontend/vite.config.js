import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  base: '/static/complex/',
  build: {
    outDir: '../static/complex',
    emptyOutDir: true,
    rollupOptions: {
      output: {
        // fixed names so the askama template doesn't need to know
        // about hashed filenames
        entryFileNames: 'bundle.js',
        assetFileNames: 'bundle.[ext]',
      },
    },
  },
  server: {
    // during `npm run dev`, forward API calls to the axum server
    proxy: {
      '/api': 'http://localhost:3000',
    },
  },
});
