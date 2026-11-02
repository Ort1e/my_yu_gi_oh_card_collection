<script>
    // The reserve: a search among the owned cards that are not in the deck.
    // The results are a drop zone; moves in and out of it are done by
    // App.svelte, which shares `cards` with this component.
    import DeckZone from './DeckZone.svelte';
    import { banStatus, outOfFormat, sortCards } from '../deck.js';

    export let api;
    export let cardTypes;
    /** the search results */
    export let cards;
    /** ids of the cards in the deck zones (saved or not), left out of the results */
    export let deckCardIds;
    export let rules = null;
    export let selectedId = null;

    const NO_FILTER = 'Please enter at least one filter to search cards.';

    /** with a format: only its legal cards (in its pool, not forbidden) */
    let legalOnly = false;

    $: shown = rules?.cutoff && legalOnly
        ? cards.filter((c) => !outOfFormat(c, rules) && banStatus(c, rules) !== 'Banned')
        : cards;

    let filters = { name: '', code: '', card_type: '' };
    let message = NO_FILTER;

    let timer;
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

            // cards moved into the deck but not saved yet are still in the results
            cards = sortCards(found.filter((c) => !deckCardIds.has(c.id)));
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
