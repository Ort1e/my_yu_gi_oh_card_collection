<script lang="ts">
    // Take the deck elsewhere: copy it as YDKE, download it as a .ydk file,
    // open a community site to import it in. Computed from the zones as
    // they are on the screen, so unsaved changes are included.
    import { ydkFile, ydkeUrl } from '../deck';
    import { toast } from './toast';
    import type { ApiCard } from '../api-types/ApiCard';
    import type { DeckZones } from '../types';

    export let zones: DeckZones;
    /** used for the .ydk file's name */
    export let name: string;

    const VARIANTS: [string, (card: ApiCard) => boolean][] = [
        ['with proxies', () => true],
        ['without proxies', (c) => !c.is_proxy],
        ['only proxies', (c) => c.is_proxy],
    ];

    async function copy(label: string, keep: (card: ApiCard) => boolean) {
        try {
            await navigator.clipboard.writeText(ydkeUrl(zones, keep));
            toast.ok(`YDKE copied (${label})`);
        } catch (e) {
            toast.error(`Failed to copy: ${e.message}`);
        }
    }

    function download(label: string, keep: (card: ApiCard) => boolean) {
        const { text, skipped } = ydkFile(zones, keep);
        const url = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
        const link = document.createElement('a');

        link.href = url;
        link.download = `${name.replace(/[^\w.-]+/g, '_') || 'deck'}.ydk`;
        link.click();
        URL.revokeObjectURL(url);

        toast.ok(`.ydk downloaded (${label})`);

        if (skipped > 0) toast.error(`${skipped} card(s) left out of the .ydk: no passcode.`);
    }
</script>

<div class="export">
    <p class="title">Copy YDKE</p>
    <div class="row">
        {#each VARIANTS as [label, keep]}
            <button type="button" class="btn-secondary" on:click={() => copy(label, keep)}>{label}</button>
        {/each}
    </div>

    <p class="title">Download a .ydk file</p>
    <div class="row">
        <button type="button" class="btn-secondary" on:click={() => download('with proxies', VARIANTS[0][1])}>with proxies</button>
        <button type="button" class="btn-secondary" on:click={() => download('without proxies', VARIANTS[1][1])}>without proxies</button>
    </div>

    <p class="title">Play or share</p>
    <p class="link">
        <a href="https://www.duelingbook.com/deck" target="_blank" rel="noopener">DuelingBook ↗</a>
        <span class="hint">Deck constructor: use its import with the .ydk file.</span>
    </p>
    <p class="link">
        <a href="https://ygoprodeck.com/deckbuilder/" target="_blank" rel="noopener">YGOPRODeck deck builder ↗</a>
        <span class="hint">Import the copied YDKE (or the .ydk file) there.</span>
    </p>
    <p class="hint">EDOPro and YGO Omega also read the .ydk file.</p>
</div>

<style>
    .title {
        margin: 0.6rem 0 0.25rem;
        font-weight: bold;
    }

    .title:first-child {
        margin-top: 0;
    }

    .row {
        display: flex;
        flex-wrap: wrap;
        gap: 0.4rem;
    }

    .link {
        margin: 0.2rem 0;
    }
</style>
