<script lang="ts">
    // The To buy tab (was Missing): what is still missing to play the deck for real: its proxies, grouped,
    // with the errata warnings, a link to buy each on Cardmarket, the
    // estimated cost and the Cardmarket wants list.
    import { createEventDispatcher } from 'svelte';

    import { cardmarketUrl, euros, missingLines } from '../deck';
    import type { ApiCard } from '../api-types/ApiCard';
    import type { Check } from '../api-types/Check';
    import type { Zone } from '../api-types/Zone';
    import type { DeckZones } from '../types';
    import { toast } from './toast';

    export let zones: DeckZones;
    export let check: Check;
    export let dirty: boolean;

    const dispatch = createEventDispatcher<{ replace: ApiCard; add: { name: string; zone: Zone } }>();

    const ZONE_NAMES: Record<Zone, string> = { main: 'Main', extra: 'Extra', side: 'Side' };

    let newName = '';
    let newZone: Zone = 'main';

    $: lines = missingLines(zones);
    $: missingCount = lines.reduce((sum, l) => sum + l.count, 0);
    $: unpriced = lines.filter((l) => l.cost == null).reduce((sum, l) => sum + l.count, 0);
    $: cost = lines.reduce((sum, l) => sum + (l.cost ?? 0), 0);

    function add() {
        if (!newName.trim()) return;

        dispatch('add', { name: newName.trim(), zone: newZone });
        newName = '';
    }

    async function copyWants() {
        try {
            await navigator.clipboard.writeText(check.wants);
            toast.ok('Wants list copied');
        } catch (e) {
            toast.error(`Failed to copy: ${e.message}`);
        }
    }
</script>

<div class="missing">
    <h3>To buy: {missingCount} card{missingCount === 1 ? '' : 's'}</h3>

    {#if missingCount === 0}
        <p>Nothing missing: every card of this version is a copy you own.</p>
    {:else}
        <p class="cost">
            About <strong>{euros(cost)}</strong> on Cardmarket (trend prices){#if unpriced > 0}, {unpriced} card{unpriced === 1 ? '' : 's'} without a price{/if}.
        </p>

        <table>
            <tbody>
                {#each lines as line (line.zone + line.card.id)}
                    <tr>
                        <td class="count">{line.count} ×</td>
                        <td>{line.card.name_en ?? line.card.name} <span class="hint">({ZONE_NAMES[line.zone]})</span></td>
                        <td class="price">{line.cost == null ? '—' : euros(line.cost)}</td>
                        <td><a href={cardmarketUrl(line.card)} target="_blank" rel="noopener">Buy ↗</a></td>
                        <td>
                            <button
                                type="button"
                                class="btn-small btn-secondary"
                                title="Put an owned copy in place of a proxy"
                                on:click={() => dispatch('replace', line.card)}>Use my copy</button
                            >
                        </td>
                    </tr>
                {/each}
            </tbody>
        </table>
    {/if}

    <form class="add" on:submit|preventDefault={add}>
        <input type="text" bind:value={newName} placeholder="Name of a missing card" />
        <select bind:value={newZone}>
            {#each Object.entries(ZONE_NAMES) as [zone, label]}<option value={zone}>{label}</option>{/each}
        </select>
        <button type="submit">Add a missing card</button>
    </form>

    {#if check.errata.length > 0}
        <h3>⚠ Text changed since the cutoff</h3>
        <ul class="errata-list">
            {#each check.errata as card}
                <li>
                    {card.count} × {card.name}
                    {#if card.wrong_text.length > 0}(owned with the wrong text: {card.wrong_text.join(', ')}){/if}
                    {#if card.warning}
                        <details>
                            <summary>{card.warning.format}: errata'd in {card.warning.changed_in}, buy {card.warning.right_sets}</summary>
                            <p><strong>Text at the cutoff</strong>: {card.warning.old_text}</p>
                            <p><strong>Text today</strong>: {card.warning.new_text}</p>
                        </details>
                    {/if}
                </li>
            {/each}
        </ul>
    {/if}

    {#if check.errata_pending > 0}
        <p class="hint">Errata being looked up on Yugipedia for {check.errata_pending} card(s): reload in a minute.</p>
    {/if}

    <h3>Cardmarket wants list</h3>

    {#if check.wants}
        {#if dirty}<p class="hint">As saved: save the deck to update it.</p>{/if}
        <textarea readonly rows={check.wants.split('\n').length + 1} value={check.wants}></textarea>
        <p class="buttons">
            <button type="button" on:click={copyWants}>Copy</button>
            <a class="btn btn-secondary" href="https://www.cardmarket.com/en/YuGiOh/Wants" target="_blank" rel="noopener">Open my Cardmarket wants lists ↗</a>
        </p>
        <p class="hint">
            Paste it into a Cardmarket Yu-Gi-Oh! wants list, then set <strong>Language: English, French</strong> and
            <strong>Min. condition: Good</strong> on its cards.
        </p>
    {:else}
        <p class="hint">No proxy: nothing to put in a wants list.</p>
    {/if}

    <p><a href="/decks/built">The shopping list over all the built decks</a></p>
</div>

<style>
    .missing {
        color: var(--text-light);
    }

    h3 {
        margin: 1rem 0 0.4rem;
    }

    h3:first-child {
        margin-top: 0;
    }

    table {
        border-collapse: collapse;
    }

    td {
        padding: 0.2rem 0.6rem 0.2rem 0;
    }

    .count,
    .price {
        text-align: right;
        font-variant-numeric: tabular-nums;
    }

    .add {
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
        margin-top: 0.8rem;
    }

    .add input {
        flex: 1 1 14rem;
        max-width: none;
        margin: 0;
    }

    .add select {
        width: auto;
        margin: 0;
    }

    textarea {
        width: 100%;
        max-width: none;
        font-family: monospace;
    }

    .buttons {
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
        align-items: center;
    }

    .errata-list {
        margin: 0;
        padding-left: 1.2rem;
    }

    summary {
        cursor: pointer;
    }
</style>
