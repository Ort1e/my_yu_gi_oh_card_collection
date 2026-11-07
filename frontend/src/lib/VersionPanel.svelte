<script lang="ts">
    // The Settings tab: name, ban list or format, built, deck box and
    // sleeves, shown on the public site, the tournament, export, delete.
    // The ban list or format is saved with the zones (Save in the header);
    // everything else saves at once, with a toast. Building changes the
    // zones, so the new version goes back to App.svelte ("reload" event).
    import { createEventDispatcher } from 'svelte';

    import { frenchDate } from '../deck';
    import type { DeckApi } from '../api';
    import type { DeckVersion } from '../api-types/DeckVersion';
    import type { DeckZones } from '../types';
    import { askYes, inform } from './dialog';
    import ExportActions from './ExportActions.svelte';
    import Tournament from './Tournament.svelte';
    import { toast } from './toast';

    export let api: DeckApi;
    /** the loaded version (shared with App.svelte: a rename updates the header) */
    export let deck: DeckVersion;
    /** the ban list or format chosen: "12", "f3", null */
    export let key: string | null;
    export let zones: DeckZones;
    export let dirty: boolean;

    const dispatch = createEventDispatcher<{ reload: DeckVersion; go: string }>();

    let name = deck.version_name;

    async function saveName() {
        const trimmed = name.trim();

        if (!trimmed || trimmed === deck.version_name) {
            name = deck.version_name;

            return;
        }

        try {
            const updated = await api.update({ version_name: trimmed });

            deck.version_name = updated.version_name;
            deck.versions = updated.versions;
            deck = deck;
            toast.ok('Renamed');
        } catch (e) {
            name = deck.version_name;
            toast.error(`Rename failed: ${e.message}`);
        }
    }

    async function toggleBuilt(e: Event & { currentTarget: HTMLInputElement }) {
        const build = e.currentTarget.checked;

        // the switch shows the saved state until the server answers
        e.currentTarget.checked = deck.is_built;

        if (dirty) {
            toast.error('Save the deck first: building works on the saved cards.');

            return;
        }

        const sure = await askYes({
            title: build ? 'Build this version?' : 'Unbuild this version?',
            message: build
                ? 'Its cards become its own: the ones shared with other decks become proxies there.'
                : 'Its cards go back to the collection. The proxies made in other decks stay until clean-cards puts the cards back.',
            confirmLabel: build ? 'Build' : 'Unbuild',
        });

        if (!sure) return;

        try {
            const updated = await api.update({ is_built: build });

            dispatch('reload', updated);

            const report = updated.build_report;

            if (report && report.proxied_here.length + report.proxied_elsewhere.length > 0) {
                const lines: string[] = [];

                if (report.proxied_here.length > 0) {
                    lines.push('Already in another built deck, now proxies here:', ...report.proxied_here.map((l) => `• ${l}`), '');
                }

                if (report.proxied_elsewhere.length > 0) {
                    lines.push('Now proxies in the other decks:', ...report.proxied_elsewhere.map((l) => `• ${l}`));
                }

                await inform({ title: build ? 'Version built' : 'Version unbuilt', lines });
            } else {
                toast.ok(build ? 'Built' : 'Unbuilt');
            }
        } catch (err) {
            toast.error(`Could not change the build: ${err.message}`);
        }
    }

    async function togglePublic(e: Event & { currentTarget: HTMLInputElement }) {
        const shown = e.currentTarget.checked;

        try {
            const updated = await api.update({ is_public: shown });

            deck.is_public = updated.is_public;
            deck = deck;
            toast.ok(shown ? 'Shown on the public site' : 'Hidden from the public site');
        } catch (err) {
            e.currentTarget.checked = deck.is_public;
            toast.error(`Could not save it: ${err.message}`);
        }
    }

    async function saveNote(field: 'deck_box' | 'sleeves', value: string) {
        if ((deck[field] ?? '') === value.trim()) return;

        try {
            const updated = await api.update({ [field]: value });

            deck[field] = updated[field];
            toast.ok('Saved');
        } catch (err) {
            toast.error(`Could not save it: ${err.message}`);
        }
    }

    async function remove() {
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
</script>

<div class="settings">
    <section>
        <label for="version-name">Name</label>
        <input id="version-name" type="text" bind:value={name} on:change={saveName} />
    </section>

    <section>
        <label for="ban-list-select">Ban list or format</label>
        <select id="ban-list-select" bind:value={key} on:change={() => (dirty = true)}>
            {#if deck.formats.length > 0}
                <optgroup label="Formats">
                    {#each deck.formats as format (format.id)}
                        <option value="f{format.id}">{format.label} (cards up to {frenchDate(format.cutoff)})</option>
                    {/each}
                </optgroup>
            {/if}
            <optgroup label="Konami's lists">
                {#each deck.ban_lists as banList, i (banList.id)}
                    <option value={String(banList.id)}>{frenchDate(banList.date)}{i === 0 ? ' (latest)' : ''}</option>
                {/each}
            </optgroup>
            <option value={null}>No ban list</option>
        </select>
        <span class="hint">
            Limits shown on the cards. Saved with the deck (Save, in the header).
            <a href="/formats" target="_blank" rel="noopener">The formats</a>
        </span>
    </section>

    <section>
        <label class="switch" title="A built version locks its real cards: they are out of the collection for the other decks">
            <input type="checkbox" checked={deck.is_built} on:change={toggleBuilt} />
            Built (locks its cards)
        </label>
        <span class="hint">
            A built version's real cards are out of the collection for every other deck, which get proxies instead.
            Build or unbuild on the saved cards.
        </span>
    </section>

    <section class="notes">
        <label>
            Deck box
            <input type="text" value={deck.deck_box ?? ''} placeholder="red Boulder 100+" on:change={(e) => saveNote('deck_box', e.currentTarget.value)} />
        </label>
        <label>
            Sleeves
            <input type="text" value={deck.sleeves ?? ''} placeholder="Dragon Shield matte black" on:change={(e) => saveNote('sleeves', e.currentTarget.value)} />
        </label>
    </section>

    <section>
        {#if deck.tournament}
            <p class="hint">On the public site: played in a tournament.</p>
        {:else}
            <label class="switch">
                <input type="checkbox" checked={deck.is_public} on:change={togglePublic} />
                Show on the public site
            </label>
        {/if}
        <a href="/deck-versions/{deck.id}" target="_blank" rel="noopener">The read-only page ↗</a>
    </section>

    <section>
        <h3>Tournament</h3>
        <Tournament deckVersionId={deck.id} tournament={deck.tournament} />
    </section>

    <section>
        <h3>Export / Play</h3>
        <ExportActions {zones} name={`${deck.deck_name ?? 'deck'}-${deck.version_name}`} />
    </section>

    <details class="danger-zone">
        <summary>Danger zone</summary>
        <button type="button" class="btn-danger" on:click={remove}>Delete this version</button>
    </details>
</div>

<style>
    .settings {
        display: flex;
        flex-direction: column;
        gap: 1.2rem;
        max-width: 42rem;
        color: var(--text-light);
    }

    section {
        display: flex;
        flex-direction: column;
        gap: 0.4rem;
    }

    h3 {
        margin: 0;
    }

    input[type='text'],
    select {
        max-width: none;
    }

    .notes {
        flex-direction: row;
        flex-wrap: wrap;
        gap: 0.5rem 1rem;
    }

    .notes label {
        flex: 1 1 12rem;
    }

    .danger-zone summary {
        cursor: pointer;
        margin-bottom: 0.5rem;
    }
</style>
