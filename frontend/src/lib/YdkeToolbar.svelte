<script>
    // Under the title: import a YDKE string as a new version of the deck
    // (replacing the current version when it is empty), and copy the deck as
    // YDKE (computed here from the zones, so unsaved changes are included).
    import { importYdke } from '../api.js';
    import { ydkeUrl } from '../deck.js';

    /** the deck to import into; null for a version without a deck */
    export let deckId;
    export let zones;
    /** the current version is empty: deleted once the import is done */
    export let replaceable = false;
    /** deletes the current version */
    export let removeVersion;

    const VARIANTS = {
        'With Proxies': () => true,
        'Without Proxies': (c) => !c.is_proxy,
        'Only Proxies': (c) => c.is_proxy,
    };

    let open = false;
    let copied = '';

    async function doImport() {
        if (!deckId) return alert('This version has no deck to import into.');

        const ydke = prompt('Paste your YDKE string:');

        if (!ydke?.trim()) return;

        const question = replaceable
            ? 'Name of the new version (it replaces this empty one):'
            : 'Name of the new version:';
        const name = prompt(question, 'Imported');

        if (name === null) return;

        let id;

        try {
            ({ id } = await importYdke(deckId, ydke.trim(), name));
        } catch (e) {
            return alert(`Import failed: ${e.message}`);
        }

        if (replaceable) {
            try {
                await removeVersion();
            } catch (e) {
                alert(`The deck was imported, but the empty version could not be deleted: ${e.message}`);
            }
        }

        location.href = `/deck-builder/${id}`;
    }

    async function copy(label) {
        open = false;

        try {
            await navigator.clipboard.writeText(ydkeUrl(zones, VARIANTS[label]));
            copied = label;
            setTimeout(() => (copied = ''), 2000);
        } catch (e) {
            alert(`Failed to copy: ${e.message}`);
        }
    }
</script>

<svelte:window on:click={() => (open = false)} />

<div class="toolbar">
    <button type="button" class="btn-secondary" on:click={doImport}>Import YDKE</button>

    <span class="copy">
        <button type="button" class="btn-secondary" on:click|stopPropagation={() => (open = !open)}>
            {copied ? `Copied ${copied}!` : 'Copy YDKE ▾'}
        </button>

        {#if open}
            <div class="popup">
                {#each Object.keys(VARIANTS) as label}
                    <button type="button" on:click|stopPropagation={() => copy(label)}>{label}</button>
                {/each}
            </div>
        {/if}
    </span>
</div>

<style>
    .toolbar {
        display: flex;
        justify-content: center;
        gap: 0.75rem;
        margin: -1rem 0 1rem;
    }

    .copy {
        position: relative;
    }

    /* The "Copy YDKE" choices: a small menu under the button. */
    .popup {
        position: absolute;
        top: calc(100% + 4px);
        left: 0;
        z-index: 20;
        display: flex;
        flex-direction: column;
        min-width: 100%;
        padding: 4px;
        border: 1px solid #ccc;
        border-radius: 6px;
        background: #fff;
        box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2);
    }

    .popup button {
        padding: 6px 10px;
        border-radius: 4px;
        background: none;
        color: var(--primary);
        font-weight: normal;
        text-align: left;
        white-space: nowrap;
    }

    .popup button:hover {
        background: #e3e8ff;
    }
</style>
