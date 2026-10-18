<script>
    // Under the title: import a YDKE string as a new version of the deck, and
    // copy the deck as YDKE (computed here from the zones, so unsaved changes
    // are included).
    import { importYdke } from '../api.js';
    import { ydkeUrl } from '../deck.js';

    /** the deck to import into; null for a version without a deck */
    export let deckId;
    export let zones;

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

        const name = prompt('Name of the new version:', 'Imported');

        if (name === null) return;

        try {
            const { id } = await importYdke(deckId, ydke.trim(), name);
            location.href = `/deck-builder/${id}`;
        } catch (e) {
            alert(`Import failed: ${e.message}`);
        }
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
