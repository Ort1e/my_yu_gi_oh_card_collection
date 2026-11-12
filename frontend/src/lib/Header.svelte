<script lang="ts">
    // The sticky header: deck name, the version switcher (its last entry
    // makes a new version), the badges, the counts and the price, Save and
    // Discard while there are unsaved changes, and the ⋯ menu (Rename, the
    // read-only page, Export / Play, a tournament with this version, Delete:
    // each exists only here). Also: owned/missing counts, the month's budget
    // left, the tournament the list was played at.
    import { createEventDispatcher } from 'svelte';

    import { euros } from '../deck';
    import type { DeckApi } from '../api';
    import type { DeckVersion } from '../api-types/DeckVersion';
    import type { DeckZones } from '../types';
    import { askText, askYes } from './dialog';
    import ExportActions from './ExportActions.svelte';
    import NewVersionDialog from './NewVersionDialog.svelte';
    import { toast } from './toast';

    export let api: DeckApi;
    export let deck: DeckVersion;
    export let zones: DeckZones;
    export let dirty: boolean;
    export let total: number;
    /** proxies in the zones as they are on the screen */
    export let missingCount: number;
    /** "Edison v2" or "List of 01/04/2026", null without rules */
    export let rulesLabel: string | null;

    const dispatch = createEventDispatcher<{ save: void; discard: void; go: string; renamed: void }>();

    const NEW = 'new';

    let menuOpen = false;
    let newVersion: NewVersionDialog;

    $: counts = `${zones.main.length} / ${zones.extra.length} / ${zones.side.length}`;
    $: cards = zones.main.length + zones.extra.length + zones.side.length;
    $: budgetLeft = deck.budget_left == null ? null : Number(deck.budget_left);

    async function switchVersion(e: Event & { currentTarget: HTMLSelectElement }) {
        const value = e.currentTarget.value;

        // back to the current version until the switch is done
        e.currentTarget.value = String(deck.id);

        if (value === NEW) {
            newVersion.open();

            return;
        }

        if (dirty) {
            const go = await askYes({
                title: 'Unsaved changes',
                message: 'Switching version discards the unsaved changes. Switch anyway?',
                confirmLabel: 'Discard and switch',
                danger: true,
            });

            if (!go) return;
        }

        dispatch('go', `/deck-builder/${value}${location.hash}`);
    }

    async function rename() {
        menuOpen = false;

        const name = await askText({ title: 'Rename the version', default: deck.version_name, confirmLabel: 'Rename' });

        if (!name) return;

        try {
            const updated = await api.update({ version_name: name });

            deck.version_name = updated.version_name;
            deck.versions = updated.versions;
            deck = deck;
            toast.ok('Renamed');
            dispatch('renamed');
        } catch (e) {
            toast.error(`Rename failed: ${e.message}`);
        }
    }

    async function remove() {
        menuOpen = false;

        const sure = await askYes({
            title: 'Delete this version?',
            message: `Delete the version "${deck.version_name}"? Its proxies go with it.`,
            confirmLabel: 'Delete',
            danger: true,
        });

        if (!sure) return;

        try {
            const { deck_id } = await api.remove();

            dispatch('go', deck_id ? `/decks/${deck_id}` : '/decks');
        } catch (e) {
            toast.error(`Delete failed: ${e.message}`);
        }
    }

    // the read-only page shows the saved cards
    async function view(e: MouseEvent) {
        if (!dirty) return;

        e.preventDefault();

        const go = await askYes({
            title: 'Unsaved changes',
            message: 'The read-only page shows the saved deck: unsaved changes are not included. Open it anyway?',
            confirmLabel: 'Open',
        });

        if (go) window.open(`/deck-versions/${deck.id}`, '_blank');
    }
</script>

<svelte:window on:click={() => (menuOpen = false)} />

<header class="builder-header">
    <div class="who">
        {#if deck.deck_id}
            <a class="deck-name" href="/decks/{deck.deck_id}">{deck.deck_name ?? 'No Deck'}</a>
        {:else}
            <span class="deck-name">{deck.deck_name ?? 'No Deck'}</span>
        {/if}
        <span class="sep">▸</span>

        <select aria-label="Version" value={String(deck.id)} on:change={switchVersion}>
            {#each deck.versions as version (version.id)}
                <option value={String(version.id)}>{version.version_name}</option>
            {/each}
            <option value={NEW}>+ New version…</option>
        </select>
    </div>

    <div class="badges">
        {#if deck.is_built}<span class="badge" title="It holds copies of your collection: out of it for the other decks">Built</span>{/if}
        {#if deck.is_public || deck.tournament}<span class="badge" title="Shown on the public site">Public</span>{/if}
        {#if rulesLabel}<span class="badge rules">{rulesLabel}</span>{/if}
    </div>

    <div class="numbers" title="Main / Extra / Side">
        {counts} · {euros(total)}
    </div>

    <div class="numbers" title="Copies you own and cards still to buy (proxies)">
        {cards - missingCount} owned ·
        {#if missingCount > 0}<a class="to-buy" href="#to-buy">{missingCount} to buy</a>{:else}nothing to buy{/if}
        {#if budgetLeft != null}
            · <a class="to-buy" href="/budget" target="_blank" rel="noopener" title="This month's budget">{budgetLeft >= 0 ? `${euros(budgetLeft)} left` : `${euros(-budgetLeft)} over`} this month</a>
        {/if}
    </div>

    {#if deck.tournament}
        <div class="numbers">
            <a class="to-buy" href="/tournaments/{deck.tournament.id}" target="_blank" rel="noopener">Played at {deck.tournament.name}</a>
        </div>
    {/if}

    <div class="actions">
        {#if dirty}
            <span class="unsaved" title="Unsaved changes">●</span>
            <button type="button" on:click={() => dispatch('save')}>Save</button>
            <button type="button" class="btn-secondary" on:click={() => dispatch('discard')}>Discard</button>
        {/if}

        <span class="menu">
            <button type="button" class="btn-secondary" aria-label="Menu" on:click|stopPropagation={() => (menuOpen = !menuOpen)}>⋯</button>

            {#if menuOpen}
                <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
                <div class="popup" on:click|stopPropagation>
                    <button type="button" class="item" on:click={rename}>Rename</button>
                    <a class="item" href="/deck-versions/{deck.id}" target="_blank" rel="noopener" on:click={view}>Read-only page ↗</a>

                    <div class="section">
                        <ExportActions {zones} name={`${deck.deck_name ?? 'deck'}-${deck.version_name}`} />
                    </div>

                    <a class="item" href="/tournaments/add?version={deck.id}" target="_blank" rel="noopener">+ Tournament with this version ↗</a>

                    <button type="button" class="item danger" on:click={remove}>Delete this version</button>
                </div>
            {/if}
        </span>
    </div>
</header>

<NewVersionDialog bind:this={newVersion} {api} {deck} {dirty} on:created={({ detail }) => dispatch('go', `/deck-builder/${detail}`)} />

<style>
    .builder-header {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 0.5rem 1.2rem;
        padding: 0.5rem 1rem;
        border-radius: 10px;
        background: var(--secondary);
        color: var(--text-light);
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    }

    .who {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        font-size: 1.15rem;
        font-weight: bold;
    }

    .deck-name {
        color: var(--text-light);
    }

    select {
        width: auto;
        max-width: 16rem;
        margin: 0;
    }

    .badge {
        padding: 0.1rem 0.5rem;
        border-radius: 999px;
        background: rgba(0, 0, 0, 0.3);
        font-size: 0.85rem;
    }

    .badges {
        display: flex;
        gap: 0.4rem;
    }

    .numbers {
        font-variant-numeric: tabular-nums;
    }

    .to-buy {
        color: var(--text-light);
        text-decoration: underline;
    }

    .actions {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-left: auto;
    }

    .unsaved {
        color: #ffd166;
        font-size: 1.2rem;
    }

    .menu {
        position: relative;
    }

    .popup {
        position: absolute;
        top: calc(100% + 4px);
        right: 0;
        z-index: 30;
        display: flex;
        flex-direction: column;
        gap: 0.2rem;
        width: 22rem;
        max-width: 90vw;
        padding: 0.6rem;
        border-radius: 8px;
        background: #fff;
        color: #222;
        font-size: 0.95rem;
        font-weight: normal;
        box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
    }

    .item {
        padding: 6px 10px;
        border-radius: 4px;
        background: none;
        color: var(--primary);
        text-align: left;
    }

    .item:hover {
        background: #e3e8ff;
    }

    .item.danger {
        color: var(--danger);
    }

    .section {
        margin: 0.3rem 0;
        padding: 0.5rem 0;
        border-top: 1px solid #ddd;
        border-bottom: 1px solid #ddd;
    }
</style>
