<script>
    import { createEventDispatcher, onDestroy } from 'svelte';
    import { Chart, registerables } from 'chart.js';
    import CardTile from './CardTile.svelte';
    import { draw, sortCards } from '../deck.js';

    Chart.register(...registerables);

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
    let newCategory = '';
    let canvas;
    let chart;

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

    async function addCategory() {
        const name = newCategory.trim();

        if (!name) return;

        try {
            const category = await api.addCategory(name);
            newCategory = '';
            dispatch('categories', [...categories, category]);
        } catch (e) {
            alert(`Could not add the category: ${e.message}`);
        }
    }

    async function removeCategory(category) {
        if (!confirm(`Delete the category "${category.name}"?`)) return;

        try {
            await api.deleteCategory(category.id);
            dispatch('categories', categories.filter((c) => c.id !== category.id));
        } catch (e) {
            alert(`Could not delete the category: ${e.message}`);
        }
    }

    // stats of a category in the last result, if any
    $: stats = new Map((result?.categories ?? []).map((s) => [s.category_id, s]));
    $: maxInHand = result ? Math.max(0, ...result.categories.flatMap((s) => Object.keys(s.per_hand).map(Number))) : 0;
    const percent = (n) => ((n / result.total_simulations) * 100).toFixed(2);

    $: if (canvas && result) drawChart(result, categories);

    function drawChart(result, categories) {
        chart?.destroy();
        const labels = Array.from({ length: maxInHand + 1 }, (_, i) => String(i));
        chart = new Chart(canvas, {
            type: 'line',
            data: {
                labels,
                datasets: categories.map((category, i) => {
                    const s = stats.get(category.id);
                    const color = `hsl(${(i * 60) % 360}, 70%, 50%)`;

                    return {
                        label: category.name,
                        data: labels.map((n) => (s ? percent(s.per_hand[n] ?? 0) : 0)),
                        borderColor: color,
                        backgroundColor: color,
                        tension: 0.2,
                    };
                }),
            },
            options: {
                responsive: true,
                interaction: { mode: 'index', intersect: false },
                plugins: {
                    title: { display: true, text: 'Hands with N cards of each category (%)' },
                    tooltip: { callbacks: { label: (c) => `${c.dataset.label}: ${c.parsed.y}%` } },
                },
                scales: {
                    x: { title: { display: true, text: 'Number of cards in hand' } },
                    y: { title: { display: true, text: 'Hands (%)' }, min: 0, max: 100 },
                },
            },
        });
    }

    onDestroy(() => chart?.destroy());
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
            <td>{category.name}{category.is_conditional ? ' (conditional)' : ''}</td>
            <td>{s ? s.cards_in_deck : 'No info'}</td>
            <td>{s ? s.occurrences : 'No info'}</td>
            <td>{s ? (s.occurrences / result.total_simulations).toFixed(2) : 'No info'}</td>
            <td><button type="button" class="remove-cat-btn" on:click={() => removeCategory(category)}>✕</button></td>
            {#if result}
                {#each Array(maxInHand + 1) as _, n}
                    {@const count = s?.per_hand[n] ?? 0}
                    <td>{count} ({percent(count)}%)</td>
                {/each}
            {/if}
        </tr>
    {/each}
</table>
<form on:submit|preventDefault={addCategory}>
    <input type="text" bind:value={newCategory} placeholder="New category name" maxlength="255" />
    <button type="submit">Add Category</button>
</form>

{#if result}<div class="chart"><canvas bind:this={canvas} height="200"></canvas></div>{/if}
