<script lang="ts">
    // The card search: the owned cards, and by name the other cards the app
    // knows, each with what the collection holds of it. A result dropped in a
    // zone adds a copy of it (the result stays); App.svelte does the moves
    // and shares `cards` with this component.
    import DeckZone from './DeckZone.svelte';
    import { banStatus, outOfFormat, sortCards, tiles } from '../deck';
    import type { DeckApi } from '../api';
    import type { Rules, Tile } from '../types';

    export let api: DeckApi;
    export let cardTypes: string[];
    /** the search results */
    export let cards: Tile[];
    export let rules: Rules | null = null;
    export let selectedId: number | null | undefined = null;

    const NO_FILTER = 'Please enter at least one filter to search cards.';

    /** with a format: only its legal cards (in its pool, not forbidden) */
    let legalOnly = false;

    $: shown = rules?.cutoff && legalOnly
        ? cards.filter((c) => !outOfFormat(c, rules) && banStatus(c, rules) !== 'Banned')
        : cards;

    let filters = { name: '', code: '', card_type: '' };
    let message = NO_FILTER;

    let timer: ReturnType<typeof setTimeout> | undefined;
    let searchId = 0;

    function schedule() {
        clearTimeout(timer);
        timer = setTimeout(search, 250);
    }

    async function search() {
        const active = filters.name.trim() || filters.code.trim() || filters.card_type;

        if (!active) {
            cards = [];
            message = NO_FILTER;

            return;
        }

        const id = ++searchId;

        try {
            const found = await api.reserve(filters);

            if (id !== searchId) return; // a newer search started

            cards = sortCards(tiles(found));
            message = 'No card found.';
        } catch (e) {
            message = `Search failed: ${e.message}`;
        }
    }
</script>

<input type="text" placeholder="Search cards..." bind:value={filters.name} on:input={schedule} />
<input type="text" placeholder="Search by code..." bind:value={filters.code} on:input={schedule} />
<select bind:value={filters.card_type} on:change={schedule}>
    <option value="">Card Type (any)</option>
    {#each cardTypes as cardType}
        <option value={cardType}>{cardType}</option>
    {/each}
</select>
{#if rules?.cutoff}
    <label class="switch legal-only"><input type="checkbox" bind:checked={legalOnly}> Legal in this format only</label>
{/if}

<div class="results">
    <DeckZone zone="reserve" cards={shown} {rules} {selectedId} on:move on:select>
        <span slot="empty" class="hint">{message}</span>
    </DeckZone>
</div>

<style>
    /* The look comes from static/css/form.css: the fields only take the whole
       width of the panel. */
    input,
    select {
        max-width: none;
        margin-bottom: 0;
    }

    .legal-only {
        margin: 0.5rem 0 0;
    }

    .results {
        max-height: 500px;
        overflow-y: auto;
        overflow-x: hidden;
        padding-right: 6px;
    }
</style>
