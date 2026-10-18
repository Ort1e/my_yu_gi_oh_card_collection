<script>
    import { createEventDispatcher } from 'svelte';
    import CardTile from './CardTile.svelte';
    import CategoryForm from './CategoryForm.svelte';
    import SimulationChart from './SimulationChart.svelte';
    import { draw, sortCards } from '../deck.js';

    export let api;
    export let main;
    export let categories;
    export let banListId = null;
    /** the deck has unsaved changes (the simulation runs on the saved deck) */
    export let dirty = false;

    const dispatch = createEventDispatcher();

    let handSize = 5;
    let simulations = 10000;
    let hand = [];
    let result = null;
    let running = false;

    function testDraw() {
        hand = sortCards(draw(main, handSize));
    }

    async function run() {
        running = true;

        try {
            result = await api.simulate(Number(handSize), Number(simulations));
        } catch (e) {
            alert(`Simulation failed: ${e.message}`);
        } finally {
            running = false;
        }
    }

    // reload from the server: deleting a category also deletes the
    // conditional categories whose rule uses it
    async function reloadCategories() {
        try {
            dispatch('categories', await api.categories());
        } catch (e) {
            alert(`Could not reload the categories: ${e.message}`);
        }
    }

    async function removeCategory(category) {
        if (!confirm(`Delete the category "${category.name}"? Conditional categories using it are deleted too.`)) return;

        try {
            await api.deleteCategory(category.id);
            result = null;
            await reloadCategories();
        } catch (e) {
            alert(`Could not delete the category: ${e.message}`);
        }
    }

    $: names = new Map(categories.map((c) => [c.id, c.name]));

    /** "if extender or lv5: starter, otherwise brick" */
    function rule({ if_any_of, then, otherwise }) {
        const ifs = if_any_of.map((id) => names.get(id)).join(' or ');

        return `if ${ifs}: ${names.get(then)}, otherwise ${otherwise ? names.get(otherwise) : 'nothing'}`;
    }

    // stats of a category in the last result, if any
    $: stats = new Map((result?.categories ?? []).map((s) => [s.category_id, s]));
    $: maxInHand = result ? Math.max(0, ...result.categories.flatMap((s) => Object.keys(s.per_hand).map(Number))) : 0;
    const percent = (n) => ((n / result.total_simulations) * 100).toFixed(2);
</script>

<div class="controls">
    <button type="button" on:click={testDraw} disabled={main.length === 0}>Draw</button>
    <label>Hand size <input type="number" bind:value={handSize} min="1" max="60" /></label>
</div>
<div class="controls">
    <button type="button" on:click={run} disabled={running}>{running ? 'Running…' : 'Run Simulation'}</button>
    <label>Simulations <input type="number" bind:value={simulations} min="1" max="1000000" /></label>
    {#if dirty}<span class="warning">Unsaved changes: the simulation uses the saved deck.</span>{/if}
</div>

<div class="dropzone">
    {#each hand as card, i (i)}
        <CardTile {card} {banListId} on:select />
    {:else}
        <span class="hint">Draw to see a test hand.</span>
    {/each}
</div>

<h4>Statistics</h4>
<table class="bordered-table">
    <tr>
        <th>Category</th>
        <th>In main deck</th>
        <th>Hits</th>
        <th>Avg/hand</th>
        <th>Remove</th>
        {#if result}{#each Array(maxInHand + 1) as _, n}<th>{n}/hand</th>{/each}{/if}
    </tr>
    {#each categories as category (category.id)}
        {@const s = stats.get(category.id)}
        <tr>
            <td>
                {category.name}
                {#if category.condition}
                    <div class="rule" title={category.condition.description}>{rule(category.condition)}</div>
                {/if}
            </td>
            <td>{s ? s.cards_in_deck : 'No info'}</td>
            <td>{s ? s.occurrences : 'No info'}</td>
            <td>{s ? (s.occurrences / result.total_simulations).toFixed(2) : 'No info'}</td>
            <td><button type="button" class="btn-small btn-danger" on:click={() => removeCategory(category)}>✕</button></td>
            {#if result}
                {#each Array(maxInHand + 1) as _, n}
                    {@const count = s?.per_hand[n] ?? 0}
                    <td>{count} ({percent(count)}%)</td>
                {/each}
            {/if}
        </tr>
    {/each}
</table>

{#if result && result.categories.length > 0}
    <SimulationChart {result} />
{/if}

<CategoryForm {api} {categories} on:added={reloadCategories} />

<style>
    .controls {
        display: flex;
        align-items: center;
        gap: 0.75rem;
    }

    .controls input {
        width: 8rem;
        padding: 8px 10px;
        font-size: 1rem;
        border: 1px solid var(--border-color);
        border-radius: 6px;
        background: var(--card-bg);
        color: var(--text-light);
    }

    .controls button {
        padding: 8px 14px;
        font-size: 1rem;
        border-radius: 6px;
        background: var(--primary);
        color: var(--text-light);
        border: none;
        cursor: pointer;
    }

    .controls button:disabled {
        opacity: 0.5;
        cursor: default;
    }

    .bordered-table {
        border-collapse: collapse;
    }

    .bordered-table th,
    .bordered-table td {
        border: 1px solid var(--border-color);
        padding: 4px 8px;
    }

    /* The rule of a conditional category, under its name in the statistics. */
    .rule {
        font-size: 0.8rem;
        font-style: italic;
        color: var(--text-muted);
    }
</style>
