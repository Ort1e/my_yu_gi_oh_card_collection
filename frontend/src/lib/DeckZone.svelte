<script>
    import { createEventDispatcher } from 'svelte';
    import CardTile from './CardTile.svelte';

    /** "main" | "extra" | "side" | "reserve" */
    export let zone;
    export let cards;
    export let banListId = null;
    export let selectedId = null;

    const dispatch = createEventDispatcher();
    let over = false;

    function drop(e) {
        e.preventDefault();
        // handled here: the window-level "dropped outside" handler must not run
        e.stopPropagation();
        over = false;
        const data = e.dataTransfer.getData('application/json');

        if (data) dispatch('move', { ...JSON.parse(data), to: zone });
    }

    function contextMenu(e) {
        if (zone === 'reserve') return;

        e.preventDefault();
        dispatch('proxy', zone);
    }
</script>

<div
    class="dropzone"
    class:drag-over={over}
    role="list"
    on:dragover|preventDefault={() => (over = true)}
    on:dragleave={() => (over = false)}
    on:drop={drop}
    on:contextmenu={contextMenu}
>
    {#each cards as card (card.id)}
        <CardTile
            {card}
            from={zone}
            {banListId}
            selected={card.id === selectedId}
            on:select
        />
    {:else}
        <slot name="empty" />
    {/each}
</div>
