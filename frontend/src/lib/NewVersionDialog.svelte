<script lang="ts">
    // "+ New version…": a name and a start, an empty version, a copy of this
    // one or the cards of a YDKE (pasted, or read from a .ydk file). The new
    // version opens in the builder.
    import { createEventDispatcher } from 'svelte';

    import { importYdke } from '../api';
    import { ydkToYdke } from '../deck';
    import type { DeckApi } from '../api';
    import type { DeckVersion } from '../api-types/DeckVersion';
    import { askYes } from './dialog';
    import { toast } from './toast';

    export let api: DeckApi;
    export let deck: DeckVersion;
    export let dirty: boolean;

    const dispatch = createEventDispatcher<{ created: number }>();

    let dialog: HTMLDialogElement;
    let name = '';
    let start: 'empty' | 'copy' | 'ydke' = 'empty';
    let ydke = '';
    let busy = false;

    export function open() {
        name = '';
        start = 'empty';
        ydke = '';
        dialog.showModal();
    }

    async function readFile(e: Event & { currentTarget: HTMLInputElement }) {
        const file = e.currentTarget.files?.[0];

        if (!file) return;

        const converted = ydkToYdke(await file.text());

        if (converted === null) {
            toast.error('No card found in this .ydk file.');
        } else {
            ydke = converted;
            start = 'ydke';
        }
    }

    async function create() {
        if (!name.trim()) {
            toast.error('Give the version a name.');

            return;
        }

        if (start === 'ydke' && !ydke.trim()) {
            toast.error('Paste a YDKE or choose a .ydk file.');

            return;
        }

        if (start === 'copy' && dirty) {
            const go = await askYes({
                title: 'Unsaved changes',
                message: 'The new version is a copy of the saved deck: unsaved changes are not included. Continue?',
                confirmLabel: 'Continue',
            });

            if (!go) return;
        }

        busy = true;

        try {
            let id: number;

            if (start === 'copy') {
                ({ id } = await api.clone(name.trim()));
            } else {
                if (deck.deck_id == null) throw new Error('this version has no deck to add a version to.');

                // an empty version is an import of nothing
                ({ id } = await importYdke(deck.deck_id, start === 'ydke' ? ydke.trim() : 'ydke://!!!', name.trim()));
            }

            dialog.close();
            dispatch('created', id);
        } catch (e) {
            toast.error(`Could not create the version: ${e.message}`);
        } finally {
            busy = false;
        }
    }
</script>

<dialog bind:this={dialog} class="builder-dialog">
    <form on:submit|preventDefault={create}>
        <h3>New version</h3>

        <label for="new-version-name">Name</label>
        <input id="new-version-name" type="text" bind:value={name} placeholder="e.g. v2, side-deck tech" />

        <fieldset>
            <label><input type="radio" bind:group={start} value="empty" /> An empty version</label>
            <label><input type="radio" bind:group={start} value="copy" /> A copy of {deck.version_name}</label>
            <label><input type="radio" bind:group={start} value="ydke" /> Cards from a YDKE or a .ydk file</label>
        </fieldset>

        {#if start === 'ydke'}
            <textarea rows="3" bind:value={ydke} placeholder="ydke://…"></textarea>
            <label>
                or a .ydk file
                <input type="file" accept=".ydk,text/plain" on:change={readFile} />
            </label>
        {/if}

        <div class="buttons">
            <button type="button" class="btn-secondary" on:click={() => dialog.close()}>Cancel</button>
            <button type="submit" disabled={busy}>Create</button>
        </div>
    </form>
</dialog>

<style>
    .builder-dialog {
        width: min(32rem, 92vw);
        padding: 1.2rem;
        border: none;
        border-radius: 12px;
        background: #fff;
        color: #222;
        box-shadow: 0 8px 30px rgba(0, 0, 0, 0.4);
    }

    .builder-dialog::backdrop {
        background: rgba(0, 0, 0, 0.5);
    }

    h3 {
        margin: 0 0 0.6rem;
        color: var(--primary);
    }

    input[type='text'],
    textarea {
        width: 100%;
        max-width: none;
        box-sizing: border-box;
    }

    fieldset {
        margin: 0.8rem 0;
        border: none;
        padding: 0;
    }

    fieldset label {
        display: block;
        margin: 0.2rem 0;
    }

    .buttons {
        display: flex;
        justify-content: flex-end;
        gap: 0.5rem;
        margin-top: 1rem;
    }
</style>
