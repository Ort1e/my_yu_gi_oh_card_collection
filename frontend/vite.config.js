import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// The deck builder app. Built into ../static/deck-builder/ and loaded by the
// /deck-builder/:id page of the backend.
export default defineConfig({
    plugins: [svelte()],
    base: '/static/deck-builder/',
    build: {
        outDir: '../static/deck-builder',
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
        // during `npm run dev`, forward backend calls to the axum server;
        // open http://localhost:5173/?id=<deck version id>
        proxy: {
            '/api': 'http://localhost:3000',
            '/card-image': 'http://localhost:3000',
            '/static/css': 'http://localhost:3000',
        },
    },
});
