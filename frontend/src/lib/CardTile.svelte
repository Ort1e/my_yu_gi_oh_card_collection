<script lang="ts">
    import { createEventDispatcher } from 'svelte';
    import { banStatus, englishName, imageUrl, outOfFormat } from '../deck';
    import type { DragData, Place, Rules, Tile } from '../types';

    export let card: Tile;
    /** zone the card is in ("main", "reserve", …); null = not draggable */
    export let from: Place | null = null;
    export let rules: Rules | null = null;
    export let selected = false;

    const dispatch = createEventDispatcher<{ select: Tile }>();

    // only when `from` is set (draggable)
    function dragStart(e: DragEvent) {
        e.dataTransfer!.setData('application/json', JSON.stringify({ cardId: card.id, from } as DragData));
        e.dataTransfer!.effectAllowed = 'move';
    }

    $: src = imageUrl(card);
    $: ban = banStatus(card, rules);
    $: out = outOfFormat(card, rules);
    $: name = englishName(card) ? `${card.name} (${englishName(card)})` : card.name;
    $: title = out ? `${name}: not in this format's card pool` : name;
</script>

<button
    type="button"
    class="deck-card"
    class:selected
    draggable={from !== null}
    data-missing={card.missing}
    data-ban_status={ban}
    class:out-of-format={out}
    {title}
    on:dragstart={dragStart}
    on:click={() => dispatch('select', card)}
>
    {#if src}
        <img {src} alt={card.name} loading="lazy" draggable="false" />
    {:else}
        <span class="no-image">{card.name}</span>
    {/if}
    {#if out}<span class="out-badge" aria-hidden="true">Out</span>{/if}
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

    /* missing cards (not owned): red border and badge (bottom-left) */
    .deck-card[data-missing="true"] {
        border: 2px solid red;
        box-shadow: 0 0 10px rgba(255, 0, 0, 0.6);
    }

    .deck-card[data-missing="true"]::after {
        content: "Missing";
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

    /* out of the format's card pool: greyed, "Out" badge (top-left: the
       missing badge is at the bottom, the limit at the top-right) */
    .deck-card.out-of-format img {
        filter: grayscale(1);
        opacity: 0.55;
    }

    .deck-card.out-of-format .out-badge {
        position: absolute;
        top: 4px;
        left: 4px;
        padding: 2px 4px;
        border-radius: 4px;
        background: #6c3483;
        color: white;
        font-size: 0.7rem;
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
