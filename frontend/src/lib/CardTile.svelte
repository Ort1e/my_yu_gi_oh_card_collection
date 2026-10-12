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
