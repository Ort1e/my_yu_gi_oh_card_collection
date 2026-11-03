import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

// `<script lang="ts">` in the components: Vite's TypeScript (esbuild) strips
// the types; `npm run check` (svelte-check) checks them.
export default {
    preprocess: vitePreprocess(),
};
