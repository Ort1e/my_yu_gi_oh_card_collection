<script>
    import { createEventDispatcher } from 'svelte';
    import { banStatus, imageUrl } from '../deck.js';

    export let card;
    /** zone the card is in ("main", "reserve", …); null = not draggable */
    export let from = null;
    export let banListId = null;
    export let selected = false;

    const dispatch = createEventDispatcher();

    function dragStart(e) {
        e.dataTransfer.setData('application/json', JSON.stringify({ cardId: card.id, from }));
        e.dataTransfer.effectAllowed = 'move';
    }

    $: src = imageUrl(card);
    $: ban = banStatus(card, banListId);
</script>

<button
    type="button"
    class="deck-card"
    class:selected
    draggable={from !== null}
    data-is_proxy={card.is_proxy}
    data-ban_status={ban}
    title={card.name}
    on:dragstart={dragStart}
    on:click={() => dispatch('select', card)}
>
    {#if src}
        <img {src} alt={card.name} loading="lazy" draggable="false" />
    {:else}
        <span class="no-image">{card.name}</span>
    {/if}
</button>

<style>
    .deck-card {
        position: relative;
        width: 65px;
        height: 90px;
        padding: 0;
        border: 1px solid var(--border-color);
        border-radius: 8px;
        overflow: hidden;
        background: var(--card-bg);
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5);
        cursor: grab;
        transition: transform 0.2s ease, box-shadow 0.2s ease;
    }

    .deck-card img {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .deck-card .no-image {
        display: block;
        padding: 4px;
        font-size: 0.6rem;
        color: var(--text-light);
        word-break: break-word;
    }

    .deck-card:active {
        cursor: grabbing;
    }

    .deck-card:hover {
        transform: scale(1.05);
        box-shadow: 0 6px 12px rgba(0, 0, 0, 0.7);
    }

    .deck-card.selected {
        outline: 3px solid var(--primary);
    }

    /* proxy cards: red border and badge (bottom-left) */
    .deck-card[data-is_proxy="true"] {
        border: 2px solid red;
        box-shadow: 0 0 10px rgba(255, 0, 0, 0.6);
    }

    .deck-card[data-is_proxy="true"]::after {
        content: "Proxy";
        position: absolute;
        bottom: 2px;
        left: 4px;
        background: rgba(255, 0, 0, 0.8);
        color: white;
        font-size: 0.7rem;
        padding: 2px 4px;
        border-radius: 4px;
        pointer-events: none;
        z-index: 10;
    }

    /* ban status badge (top-right) */
    .deck-card[data-ban_status]::before {
        position: absolute;
        top: 4px;
        right: 4px;
        color: white;
        font-size: 0.7rem;
        padding: 2px 6px;
        border-radius: 4px;
        pointer-events: none;
        z-index: 10;
    }

    .deck-card[data-ban_status="Limited"]::before {
        content: "1";
        background: #ffc107;
    }

    .deck-card[data-ban_status="Semi-Limited"]::before {
        content: "2";
        background: #17a2b8;
    }

    .deck-card[data-ban_status="Banned"]::before {
        content: "X";
        background: #dc3545;
    }
</style>
