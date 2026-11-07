<script lang="ts">
    // The deck's history: the lists saved with each save of a version's
    // zones (crates/yugioh/src/store/snapshots.rs), what changed from one of
    // them to the zones on the screen, and "Name this save" to keep one.
    import { frenchDate } from '../deck';
    import type { DeckApi } from '../api';
    import type { ApiChange } from '../api-types/ApiChange';
    import type { DeckVersion } from '../api-types/DeckVersion';
    import type { Snapshot } from '../api-types/Snapshot';
    import type { Zone } from '../api-types/Zone';
    import type { DeckZones } from '../types';
    import { toast } from './toast';

    export let api: DeckApi;
    export let deck: DeckVersion;
    export let zones: DeckZones;
    export let dirty: boolean;

    const ZONE_NAMES: Record<Zone, string> = { main: 'Main', extra: 'Extra', side: 'Side' };

    let snapshots: Snapshot[] = [];
    let loadError: string | null = null;
    let chosen: number | null = null;
    let changes: ApiChange[] | null = null;
    let newName = '';
    let saving = false;

    const title = (s: Snapshot): string =>
        s.name ?? s.tournament_name ?? (s.version_name ? `${s.version_name} (saved)` : 'Saved list');

    const when = (s: Snapshot): string => `${frenchDate(s.created_at.slice(0, 10))} ${s.created_at.slice(11, 16)}`;

    const ids = (zone: Zone): number[] => zones[zone].map((c) => c.id);

    async function load() {
        try {
            snapshots = await api.history();
            loadError = null;
        } catch (e) {
            loadError = e.message;
        }
    }

    // loaded again after each save (the deck comes back from the API)
    $: deck, load();

    async function compare(id: number, _zones: DeckZones) {
        try {
            changes = await api.compare(id, ids('main'), ids('extra'), ids('side'));
        } catch (e) {
            changes = null;
            toast.error(`Could not compare: ${e.message}`);
        }
    }

    // a list chosen, or the screen changed: compare
    $: if (chosen !== null) compare(chosen, zones);

    async function nameThisSave() {
        const name = newName.trim();

        if (!name || saving) return;

        saving = true;

        try {
            deck = await api.update({ snapshot_name: name });
            newName = '';
            toast.ok(`Saved list named “${name}”`);
            await load();
        } catch (e) {
            toast.error(`Could not name it: ${e.message}`);
        } finally {
            saving = false;
        }
    }
</script>

<div class="history">
    <h3>Name this save</h3>

    {#if dirty}
        <p class="hint">Save the deck first: the name goes on the saved list.</p>
    {:else}
        <form class="name" on:submit|preventDefault={nameThisSave}>
            <input type="text" bind:value={newName} maxlength="100" placeholder="Before the regional, side vs Snake-Eye…" />
            <button type="submit" disabled={saving || !newName.trim()}>Name it</button>
        </form>
        <p class="hint">A named list is kept; an automatic one is replaced by the next save within 30 minutes.</p>
    {/if}

    <h3>Saved lists of {deck.deck_name ?? 'this deck'}</h3>

    {#if loadError}
        <p class="error">{loadError}</p>
    {:else if snapshots.length === 0}
        <p>No list saved yet: one is saved each time the deck is.</p>
    {:else}
        <table>
            <thead>
                <tr><th>Saved</th><th>List</th><th>Version</th><th class="num">Main</th><th class="num">Extra</th><th class="num">Side</th><th /></tr>
            </thead>
            <tbody>
                {#each snapshots as s (s.id)}
                    <tr class:chosen={s.id === chosen}>
                        <td>{when(s)}</td>
                        <td>
                            <a href="/deck-snapshots/{s.id}" target="_blank" rel="noopener">{title(s)}</a>
                            {#if s.tournament_id !== null}🏆{/if}
                            {#if s.name === null && s.tournament_id === null}<span class="hint">automatic</span>{/if}
                        </td>
                        <td>{s.version_name ?? '—'}</td>
                        <td class="num">{s.main}</td>
                        <td class="num">{s.extra}</td>
                        <td class="num">{s.side}</td>
                        <td>
                            <button type="button" class="btn-small btn-secondary" on:click={() => (chosen = s.id)}>Compare</button>
                        </td>
                    </tr>
                {/each}
            </tbody>
        </table>
    {/if}

    {#if chosen !== null && changes !== null}
        <h3>From that list to the screen{dirty ? ' (not saved)' : ''}</h3>

        {#if changes.length === 0}
            <p>The same cards.</p>
        {:else}
            <table>
                <tbody>
                    {#each changes as c (c.zone + c.name)}
                        <tr class={c.after > c.before ? 'added' : 'removed'}>
                            <td class="num">{c.after > c.before ? '+' : '−'}{Math.abs(c.after - c.before)}</td>
                            <td>{c.name} <span class="hint">({ZONE_NAMES[c.zone]})</span></td>
                            <td class="num hint">{c.before} → {c.after}</td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        {/if}
    {/if}
</div>

<style>
    .history {
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

    th {
        text-align: left;
    }

    td,
    th {
        padding: 0.2rem 0.6rem 0.2rem 0;
    }

    .num {
        text-align: right;
        font-variant-numeric: tabular-nums;
    }

    tr.chosen td {
        font-weight: bold;
    }

    tr.added td:first-child {
        color: var(--success, #2e7d32);
    }

    tr.removed td:first-child {
        color: var(--danger, #c62828);
    }

    .name {
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
    }

    .name input {
        flex: 1 1 18rem;
        max-width: none;
        margin: 0;
    }

    .error {
        color: var(--danger, #c62828);
    }
</style>
