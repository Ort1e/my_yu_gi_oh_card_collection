<script lang="ts">
    import { createEventDispatcher } from 'svelte';
    import CardTile from './CardTile.svelte';
    import type { ApiCard } from '../api-types/ApiCard';
    import type { Zone } from '../api-types/Zone';
    import type { DragData, Move, Place, Rules } from '../types';

    export let zone: Place;
    export let cards: ApiCard[];
    export let rules: Rules | null = null;
    export let selectedId: number | null | undefined = null;

    const dispatch = createEventDispatcher<{ move: Move; proxy: Zone; select: ApiCard }>();
    let over = false;

    function drop(e: DragEvent) {
        e.preventDefault();
        // handled here: the window-level "dropped outside" handler must not run
        e.stopPropagation();
        over = false;
        const data = e.dataTransfer?.getData('application/json');

        if (data) dispatch('move', { ...(JSON.parse(data) as DragData), to: zone });
    }

    function contextMenu(e: MouseEvent) {
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
            {rules}
            selected={card.id === selectedId}
            on:select
        />
    {:else}
        <slot name="empty" />
    {/each}
</div>
