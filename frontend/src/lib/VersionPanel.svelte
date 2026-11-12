<script lang="ts">
    // The Settings tab, three blocks: Rules (ban list or format), Physical
    // deck (built or planned, deck box, sleeves), Sharing (public site). Rename, Export /
    // Play, the tournament and Delete are in the header's ⋯ menu.
    // The ban list or format is saved with the zones (Save in the header);
    // everything else saves at once, with a toast. Building changes the
    // zones, so the new version goes back to App.svelte ("reload" event).
    import { createEventDispatcher } from 'svelte';

    import { frenchDate } from '../deck';
    import type { DeckApi } from '../api';
    import type { DeckVersion } from '../api-types/DeckVersion';
    import { askYes, inform } from './dialog';
    import { toast } from './toast';

    export let api: DeckApi;
    /** the loaded version (shared with App.svelte: a rename updates the header) */
    export let deck: DeckVersion;
    /** the ban list or format chosen: "12", "f3", null */
    export let key: string | null;
    export let dirty: boolean;

    const dispatch = createEventDispatcher<{ reload: DeckVersion }>();

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
                ? 'It takes your free copies of its cards (best condition first): they are out of the collection for the other decks. The cards you lack stay missing. It also goes on the public site (Sharing hides it).'
                : 'Its copies go back to the collection.',
            confirmLabel: build ? 'Build' : 'Unbuild',
        });

        if (!sure) return;

        try {
            const updated = await api.update({ is_built: build });

            dispatch('reload', updated);

            const report = updated.build_report;

            if (report) {
                const lines = [`${report.copies} of your copies are in it now.`];

                if (report.missing > 0) lines.push(`${report.missing} missing: see the To buy tab.`);

                await inform({ title: 'Version built', lines });
            } else {
                toast.ok(build ? 'Built' : 'Unbuilt');
            }
        } catch (err) {
            toast.error(`Could not change the build: ${err.message}`);
        }
    }

    async function togglePlanned(e: Event & { currentTarget: HTMLInputElement }) {
        const planned = e.currentTarget.checked;

        try {
            const updated = await api.update({ is_planned: planned });

            deck.is_planned = updated.is_planned;
            deck = deck;
            toast.ok(planned ? 'Planned: its missing cards are on the shopping list' : 'No longer planned');
        } catch (err) {
            e.currentTarget.checked = deck.is_planned;
            toast.error(`Could not save it: ${err.message}`);
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
</script>

<div class="settings">
    <section>
        <h3>Rules</h3>

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
        <h3>Physical deck</h3>

        <label class="switch" title="A built version holds copies of your collection: they are out of it for the other decks">
            <input type="checkbox" checked={deck.is_built} on:change={toggleBuilt} />
            Built (holds its copies)
        </label>
        <span class="hint">
            Building takes your free copies of its cards; the others decks never lose theirs, only built decks hold copies.
            Build or unbuild on the saved cards.
        </span>

        {#if !deck.is_built}
            <label class="switch" title="A deck you mean to build: its missing cards go on the shopping list">
                <input type="checkbox" checked={deck.is_planned} on:change={togglePlanned} />
                Planned (to build)
            </label>
        {/if}

        <div class="notes">
            <label>
                Deck box
                <input type="text" value={deck.deck_box ?? ''} placeholder="red Boulder 100+" on:change={(e) => saveNote('deck_box', e.currentTarget.value)} />
            </label>
            <label>
                Sleeves
                <input type="text" value={deck.sleeves ?? ''} placeholder="Dragon Shield matte black" on:change={(e) => saveNote('sleeves', e.currentTarget.value)} />
            </label>
        </div>
    </section>

    <section>
        <h3>Sharing</h3>

        {#if deck.tournament}
            <p class="hint">On the public site: played in a tournament.</p>
        {:else}
            <label class="switch">
                <input type="checkbox" checked={deck.is_public} on:change={togglePublic} />
                Show on the public site
            </label>
            <span class="hint">Building a version shows it; hidden here, it stays hidden.</span>
        {/if}
        <a href="/deck-versions/{deck.id}" target="_blank" rel="noopener">The read-only page ↗</a>
    </section>
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
        display: flex;
        flex-direction: row;
        flex-wrap: wrap;
        gap: 0.5rem 1rem;
    }

    .notes label {
        flex: 1 1 12rem;
    }

</style>
