<script lang="ts">
    import { createEventDispatcher } from 'svelte';
    import { banStatus, englishName, frenchDate, imageUrl, outOfFormat } from '../deck';
    import type { DeckApi } from '../api';
    import type { ApiCard } from '../api-types/ApiCard';
    import type { CardCategory } from '../api-types/CardCategory';
    import type { Rules } from '../types';

    export let card: ApiCard | null;
    export let rules: Rules | null = null;
    export let api: DeckApi;
    /** bumped by the parent when categories are added/removed */
    export let categoriesVersion = 0;

    /** true for a proxy in a deck zone: it can be swapped for an owned copy */
    export let canReplace = false;

    const dispatch = createEventDispatcher<{ replaceProxy: ApiCard }>();

    let categories: CardCategory[] | null = null;
    let error: string | null = null;
    /** the categories being saved (a click waits for the answer) */
    let saving = new Set<number>();

    $: assignedCount = categories?.filter((c) => c.assigned).length ?? 0;

    // `_version`: only there so a new categoriesVersion loads them again
    async function load(cardId: number, _version: number) {
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

    // only shown with a card
    async function toggle(category: CardCategory) {
        if (saving.has(category.id)) return;

        const assigned = !category.assigned;
        saving = new Set(saving).add(category.id);

        try {
            await api.assignCategory(card!.id, category.id, assigned);
            category.assigned = assigned;
        } catch (e) {
            alert(`Could not change the category: ${e.message}`);
        } finally {
            saving.delete(category.id);
            saving = saving;
            categories = categories;
        }
    }
</script>

{#if card}
    {#if imageUrl(card)}<img src={imageUrl(card)} alt={card.name} />{/if}
    <div class="overview-text">
        <h4><a href="/cards/{card.id}" target="_blank" rel="noopener">{card.name}</a></h4>
        {#if englishName(card)}<p class="english-name">{englishName(card)}</p>{/if}
        <p>Type: {card.card_type || '—'}</p>
        <p>Code: {card.code || '—'}</p>
        <p>Price: {card.price ? `${card.price}€` : 'N/A'}</p>
        <p>Ban Status: {banStatus(card, rules) || 'Unlimited'}</p>
        <p>
            First TCG release: {card.first_release ? frenchDate(card.first_release) : 'unknown'}
            {#if outOfFormat(card, rules)}<strong class="out-of-format">· not in this format</strong>{/if}
        </p>
        <p>Rarity: {#if card.rarity}<a href="/rarities?rarity={encodeURIComponent(card.rarity)}" target="_blank" rel="noopener">{card.rarity}</a>{:else}—{/if}</p>
        <p>
            Proxy: {card.is_proxy ? 'Yes' : 'No'}
            {#if card.is_proxy && canReplace}
                <button
                    type="button"
                    class="replace-proxy"
                    title="Put an owned copy of this card in place of the proxy"
                    on:click={() => dispatch('replaceProxy', card)}>Use my copy</button
                >
            {/if}
        </p>
        <div class="category-selector">
            <p class="category-heading">
                <strong>Categories</strong>
                {#if categories && categories.length > 0}
                    <span class="category-count">{assignedCount} of {categories.length} · click to add or remove</span>
                {/if}
            </p>
            {#if error}
                <p>Error loading categories: {error}</p>
            {:else if categories === null}
                <p>Loading…</p>
            {:else if categories.length === 0}
                <p>None in this deck yet: add them under "Test Draw".</p>
            {:else}
                <div class="category-chips">
                    {#each categories as category (category.id)}
                        <label
                            class="category-chip"
                            class:checked={category.assigned}
                            class:saving={saving.has(category.id)}
                            title={category.assigned ? `In "${category.name}": click to remove` : `Not in "${category.name}": click to add`}
                        >
                            <input
                                type="checkbox"
                                checked={category.assigned}
                                disabled={saving.has(category.id)}
                                on:change={() => toggle(category)}
                            />
                            <span class="mark" aria-hidden="true">{category.assigned ? '✓' : '+'}</span>
                            <span>{category.name}</span>
                        </label>
                    {/each}
                </div>
            {/if}
        </div>
    </div>
{:else}
    <p class="overview-text">Click a card to see its details.</p>
{/if}

<style>
    .out-of-format {
        color: #ffb3b3;
    }

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

    /* The card's categories, on the green panel: a white chip with a ✓ for
       the ones it is in, a dashed outline with a + for the others. */
    .category-selector {
        margin-top: 0.75rem;
    }

    p.category-heading {
        display: flex;
        flex-wrap: wrap;
        align-items: baseline;
        gap: 0 8px;
        margin-bottom: 6px;
    }

    .category-count {
        font-size: 0.85rem;
        opacity: 0.85;
    }

    .category-chips {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
    }

    .category-chip {
        position: relative;         /* holds the hidden checkbox */
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 5px 12px;
        border: 2px dashed rgba(255, 255, 255, 0.75);
        border-radius: 20px;
        background-color: transparent;
        color: #fff;
        font-size: 0.9rem;
        cursor: pointer;
        user-select: none;
        transition: background-color 0.15s, color 0.15s;
    }

    .category-chip:hover {
        background-color: rgba(255, 255, 255, 0.2);
    }

    .category-chip.checked {
        border: 2px solid #fff;
        background-color: #fff;
        color: var(--primary);
        font-weight: bold;
    }

    .category-chip.checked:hover {
        background-color: #e6ecff;
    }

    .category-chip.saving {
        opacity: 0.6;
        cursor: progress;
    }

    /* the keyboard reaches the hidden checkbox (Tab, then Space) */
    .category-chip:has(input:focus-visible) {
        outline: 3px solid #ffd54f;
        outline-offset: 2px;
    }

    .mark {
        width: 1em;
        text-align: center;
        font-weight: bold;
    }

    /* hidden but focusable (not display: none) */
    .category-chip input {
        position: absolute;
        width: 1px;
        height: 1px;
        margin: 0;
        opacity: 0;
        pointer-events: none;
    }

    .replace-proxy {
        margin-left: 8px;
        padding: 2px 10px;
        font-size: 0.85rem;
        cursor: pointer;
    }

    /* The English name under a French one. */
    .english-name {
        margin-bottom: 6px;
        font-style: italic;
        opacity: 0.85;
    }

    /* "Click a card to see its details.": centred in the empty panel. */
    p.overview-text {
        align-self: center;
    }
</style>
