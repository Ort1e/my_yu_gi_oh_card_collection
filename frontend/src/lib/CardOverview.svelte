<script>
    import { banStatus, imageUrl } from '../deck.js';

    export let card;
    export let banListId = null;
    export let api;
    /** bumped by the parent when categories are added/removed */
    export let categoriesVersion = 0;

    let categories = null;
    let error = null;

    async function load(cardId) {
        categories = null;
        error = null;

        try {
            const list = await api.cardCategories(cardId);

            if (card?.id === cardId) categories = list;
        } catch (e) {
            error = e.message;
        }
    }

    $: if (card) load(card.id, categoriesVersion);

    async function toggle(category) {
        const assigned = !category.assigned;

        try {
            await api.assignCategory(card.id, category.id, assigned);
            category.assigned = assigned;
            categories = categories;
        } catch (e) {
            alert(`Could not change the category: ${e.message}`);
        }
    }
</script>

{#if card}
    {#if imageUrl(card)}<img src={imageUrl(card)} alt={card.name} />{/if}
    <div class="overview-text">
        <h4><a href="/cards/{card.id}" target="_blank" rel="noopener">{card.en_name || card.name}</a></h4>
        <p>Name: {card.name}</p>
        <p>Type: {card.card_type || '—'}</p>
        <p>Code: {card.code || '—'}</p>
        <p>Price: {card.price ? `${card.price}€` : 'N/A'}</p>
        <p>Ban Status: {banStatus(card, banListId) || 'Unlimited'}</p>
        <p>Proxy: {card.is_proxy ? 'Yes' : 'No'}</p>
        <div class="category-selector">
            <strong>Categories:</strong>
            {#if error}
                Error loading categories: {error}
            {:else if categories === null}
                Loading…
            {:else if categories.length === 0}
                none in this deck yet.
            {:else}
                {#each categories as category (category.id)}
                    <label class:checked={category.assigned}>
                        <input type="checkbox" checked={category.assigned} on:change={() => toggle(category)} />
                        <span>{category.name}</span>
                    </label>
                {/each}
            {/if}
        </div>
    </div>
{:else}
    <p class="overview-text">Click a card to see its details.</p>
{/if}

<style>
    img {
        width: 120px;
        height: auto;
        flex-shrink: 0;
        border-radius: 6px;
        object-fit: cover;
    }

    .overview-text {
        display: flex;
        flex-direction: column;
        justify-content: center;
        flex: 1;
        min-width: 0;               /* long names wrap instead of widening the panel */
    }

    .overview-text h4 {
        margin: 0 0 8px;
        font-size: 1.2em;
    }

    .overview-text p {
        margin: 0;
        font-size: 0.95em;
    }

    .category-selector {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        margin-top: 0.5rem;
        color: var(--text-dark);
    }

    .category-selector label {
        display: inline-flex;
        align-items: center;
        background-color: var(--hover-bg);
        border: 2px solid var(--border-color);
        border-radius: 20px;
        padding: 6px 12px;
        cursor: pointer;
        font-size: 0.9rem;
        user-select: none;
        transition: background-color 0.2s, transform 0.15s;
    }

    .category-selector label:hover {
        background-color: var(--bg-color);
    }

    .category-selector label.checked {
        border-color: var(--primary);
        font-weight: bold;
        transform: scale(1.05);
    }

    .category-selector input[type="checkbox"] {
        display: none;
    }

    /* "Click a card to see its details.": centred in the empty panel. */
    p.overview-text {
        align-self: center;
    }
</style>
