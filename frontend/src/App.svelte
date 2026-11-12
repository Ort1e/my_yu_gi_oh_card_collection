<script lang="ts">
    import { onMount } from 'svelte';
    import { deckApi, importYdke } from './api';
    import { ZONES, cardInputs, copyOf, euros, frenchDate, markMissing, rulesFields, rulesKey, rulesOf, sortCards, tiles, zonePrice } from './deck';
    import type { ApiCategory } from './api-types/ApiCategory';
    import type { Check } from './api-types/Check';
    import type { DeckVersion } from './api-types/DeckVersion';
    import type { SimulationResult } from './api-types/SimulationResult';
    import type { Zone } from './api-types/Zone';
    import type { Move, Tile, Zones } from './types';
    import CardOverview from './lib/CardOverview.svelte';
    import DeckZone from './lib/DeckZone.svelte';
    import Dialog from './lib/Dialog.svelte';
    import FormatBanner from './lib/FormatBanner.svelte';
    import Header from './lib/Header.svelte';
    import History from './lib/History.svelte';
    import Missing from './lib/Missing.svelte';
    import ReserveSearch from './lib/ReserveSearch.svelte';
    import Simulation from './lib/Simulation.svelte';
    import Toast from './lib/Toast.svelte';
    import VersionPanel from './lib/VersionPanel.svelte';
    import { askText, askYes } from './lib/dialog';
    import { toast } from './lib/toast';

    export let deckVersionId: number;

    const api = deckApi(deckVersionId);

    const ZONE_LABELS: [Zone, string][] = [
        ['main', 'Main Deck'],
        ['extra', 'Extra Deck'],
        ['side', 'Side Deck'],
    ];

    let deck: DeckVersion | null = null; // last loaded/saved version (metadata)
    let loadError: string | null = null;
    /** zone name -> cards; "reserve" holds the search results */
    let zones: Zones = { main: [], extra: [], side: [], reserve: [] };
    /** the selector's ban list or format ("12", "f3"), saved with the deck */
    let key: string | null = null;
    /** the version's problems in its format (`GET …/check`), for the panel */
    let check: Check = { format: null, problems: [], errata: [], errata_pending: 0, wants: '', wants_note: '' };
    let dirty = false;
    let selected: Tile | null = null;
    let categories: ApiCategory[] = [];
    let categoriesVersion = 0;
    /** the last test draw, for the Build tab's banner */
    let simulation: SimulationResult | null = null;

    const TABS = ['build', 'to-buy', 'draw', 'history', 'settings'] as const;
    type Tab = (typeof TABS)[number];

    // #missing, the old name of the To buy tab, still opens it
    const tabFromHash = (): Tab => (location.hash === '#missing' ? 'to-buy' : TABS.find((t) => `#${t}` === location.hash) ?? 'build');

    let tab: Tab = tabFromHash();

    function showTab(next: Tab) {
        tab = next;
        history.replaceState(null, '', `#${next}`);
    }

    function setDeck(data: DeckVersion) {
        deck = data;
        key = rulesKey(data);
        zones = {
            main: sortCards(tiles(data.main)),
            extra: sortCards(tiles(data.extra)),
            side: sortCards(tiles(data.side)),
            reserve: zones.reserve,
        };

        dirty = false;
        refreshCheck();
    }

    async function refreshCheck() {
        try {
            check = await api.check();
        } catch (e) {
            check = { format: null, problems: [`Could not check the format: ${e.message}`], errata: [], errata_pending: 0, wants: '', wants_note: '' };
        }
    }

    $: rules = deck ? rulesOf(deck, key) : null;

    // an empty version (like the "v1" made with a new deck) is replaced by a
    // YDKE import instead of staying behind
    $: isEmpty =
        !dirty &&
        deck !== null &&
        deck.main.length + deck.extra.length + deck.side.length === 0 &&
        !deck.tournament &&
        categories.length === 0;

    onMount(async () => {
        try {
            setDeck(await api.get());
            categories = await api.categories();
        } catch (e) {
            loadError = e.message;
        }
    });

    // ---------- moving cards ----------

    /** The zones with `missing` worked out again (owned copies count in zone order). */
    function remark() {
        zones = { ...markMissing(zones), reserve: zones.reserve };
    }

    /** The deck zone (not the reserve) of `zones` holding `card`, or null. */
    const zoneOf = (zones: Zones, card: Tile | null): Zone | null =>
        card ? (['main', 'extra', 'side'] as Zone[]).find((z) => zones[z].some((c) => c.id === card.id)) ?? null : null;

    $: selectedZone = zoneOf(zones, selected);

    // a built version: its empty slots take the free copies (saved at once)
    async function takeFree() {
        if (dirty) {
            toast.error('Save the deck first: the copies go to the saved cards.');

            return;
        }

        try {
            const { taken, missing } = await api.takeFree();

            setDeck(await api.get());
            selected = null;
            toast.ok(`${taken} cop${taken === 1 ? 'y' : 'ies'} taken${missing > 0 ? `, ${missing} still to buy` : ''}`);
        } catch (e) {
            toast.error(`Could not take the copies: ${e.message}`);
        }
    }

    function move({ cardId, from, to }: Move) {
        if (from === to || !zones[from]) return;

        const card = zones[from].find((c) => c.id === cardId);

        if (!card) return;

        // a search result stays in the results: a new tile of it goes in
        if (from === 'reserve') {
            if (to === 'reserve' || to === 'outside') return;

            zones[to] = sortCards([...zones[to], copyOf(card)]);
        } else {
            // out of the deck (to the results or anywhere else): gone
            zones[from] = zones[from].filter((c) => c.id !== cardId);

            if (to !== 'reserve' && to !== 'outside') zones[to] = sortCards([...zones[to], card]);

            if (selected?.id === cardId && (to === 'reserve' || to === 'outside')) selected = null;
        }

        remark();
        dirty = true;
    }

    function dropOutside(e: DragEvent) {
        e.preventDefault();
        const data = e.dataTransfer?.getData('application/json');

        if (data) move({ ...JSON.parse(data), to: 'outside' });
    }

    async function askMissing(zone: Zone) {
        const name = await askText({ title: 'Add a card', message: 'Its English name (or any name, for a card ygoprodeck lacks):', confirmLabel: 'Add' });

        if (name) await addMissing(name, zone);
    }

    async function addMissing(name: string, zone: Zone) {
        try {
            const [card] = tiles([await api.lookup(name)]);

            zones[zone] = sortCards([...zones[zone], card]);
            remark();
            dirty = true;
            toast.ok(`${card.name} added: save to keep it`);
        } catch (e) {
            toast.error(`Could not add the card: ${e.message}`);
        }
    }

    // ---------- saving ----------

    async function save() {
        try {
            const saved = await api.update({
                main: cardInputs(zones.main),
                extra: cardInputs(zones.extra),
                side: cardInputs(zones.side),
                ...rulesFields(key),
            });

            setDeck(saved);
            toast.ok('Deck saved');
        } catch (e) {
            toast.error(`Save failed: ${e.message}`);
        }
    }

    async function discard() {
        const sure = await askYes({
            title: 'Discard the changes?',
            message: 'The zones and the ban list or format go back to the saved version.',
            confirmLabel: 'Discard',
            danger: true,
        });

        if (!sure) return;

        try {
            setDeck(await api.get());
        } catch (e) {
            toast.error(`Could not reload the deck: ${e.message}`);
        }
    }

    /** leave the page (for good: no "unsaved changes" warning) */
    function go(url: string) {
        dirty = false;
        location.href = url;
    }

    // an empty version is replaced by the import instead of staying behind
    async function importIntoEmpty() {
        if (!deck || deck.deck_id == null) return;

        const ydke = await askText({
            title: 'Paste a YDKE',
            message: 'Its cards and counts become this version (the collection says which you own).',
            input: 'multiline',
            placeholder: 'ydke://…',
            confirmLabel: 'Import',
        });

        if (!ydke) return;

        let id: number;

        try {
            ({ id } = await importYdke(deck.deck_id, ydke, deck.version_name));
        } catch (e) {
            toast.error(`Import failed: ${e.message}`);

            return;
        }

        try {
            await api.remove();
        } catch (e) {
            toast.error(`The deck was imported, but the empty version could not be deleted: ${e.message}`);
        }

        go(`/deck-builder/${id}${location.hash}`);
    }

    function beforeUnload(e: BeforeUnloadEvent) {
        if (dirty) e.preventDefault();
    }

    const select = ({ detail }: CustomEvent<Tile>) => (selected = detail);

    $: total = zonePrice(zones.main) + zonePrice(zones.extra) + zonePrice(zones.side);
    $: rulesLabel = deck ? rulesLabelOf(deck, key) : null;

    function rulesLabelOf(deck: DeckVersion, key: string | null): string | null {
        if (key == null) return null;

        if (key.startsWith('f')) return deck.formats.find((f) => `f${f.id}` === key)?.label ?? null;

        const list = deck.ban_lists.find((b) => String(b.id) === key);

        return list ? `List of ${frenchDate(list.date)}` : null;
    }
    $: missingCount = ZONES.flatMap((z) => zones[z]).filter((c) => c.missing).length;
</script>


<svelte:window on:dragover|preventDefault on:drop={dropOutside} on:beforeunload={beforeUnload} on:hashchange={() => (tab = tabFromHash())} />

{#if loadError}
    <p class="error">Could not load the deck: {loadError}</p>
{:else if deck}
    <div class="builder">
        <Header
            {api}
            bind:deck
            {zones}
            {dirty}
            {total}
            {missingCount}
            {rulesLabel}
            on:save={save}
            on:discard={discard}
            on:go={({ detail }) => go(detail)}
        />

        <nav class="tabs">
            <button type="button" class:active={tab === 'build'} on:click={() => showTab('build')}>Build</button>
            <button type="button" class:active={tab === 'to-buy'} on:click={() => showTab('to-buy')}>To buy ({missingCount})</button>
            <button type="button" class:active={tab === 'draw'} on:click={() => showTab('draw')}>Test draw</button>
            <button type="button" class:active={tab === 'history'} on:click={() => showTab('history')}>History</button>
            <button type="button" class:active={tab === 'settings'} on:click={() => showTab('settings')}>Settings</button>
        </nav>

        {#if tab === 'build'}
            <div class="build">
                <div class="zones">
                    <FormatBanner {check} {dirty} result={simulation} {categories} />

                    {#if isEmpty}
                        <div class="empty">
                            <p>Drop cards from the search, or paste a YDKE.</p>
                            <button type="button" on:click={importIntoEmpty} disabled={deck.deck_id == null}>Paste a YDKE</button>
                        </div>
                    {/if}

                    {#each ZONE_LABELS as [zone, label]}
                        <div class="area">
                            <div class="zone-head">
                                <h3>{label} ({zones[zone].length}, {euros(zonePrice(zones[zone]))})</h3>
                                <button type="button" class="btn-small btn-secondary" on:click={() => askMissing(zone)} title="A card by its name, owned or not">+ Card by name</button>
                            </div>
                            <DeckZone
                                {zone}
                                cards={zones[zone]}
                                {rules}
                                selectedId={selected?.id}
                                on:move={({ detail }) => move(detail)}
                                on:missing={({ detail }) => askMissing(detail)}
                                on:select={select}
                            >
                                <span slot="empty" class="hint">Drop cards here, right-click to add a card by its name.</span>
                            </DeckZone>
                        </div>
                    {/each}
                </div>

                <aside class="side">
                    <div class="area">
                        <h3>Add cards</h3>
                        <ReserveSearch
                            {api}
                            cardTypes={deck.card_types}
                            bind:cards={zones.reserve}
                            {rules}
                            selectedId={selected?.id}
                            on:move={({ detail }) => move(detail)}
                            on:select={select}
                        />
                    </div>

                    <div id="card-overview">
                        <CardOverview
                            card={selected}
                            {rules}
                            {api}
                            {categoriesVersion}
                            canTake={deck.is_built && selectedZone !== null}
                            on:takeFree={takeFree}
                        />
                    </div>
                </aside>
            </div>
        {:else if tab === 'to-buy'}
            <div class="area">
                <Missing
                    {zones}
                    {check}
                    {dirty}
                    isBuilt={deck.is_built}
                    on:takeFree={takeFree}
                    on:add={({ detail }) => addMissing(detail.name, detail.zone)}
                />
            </div>
        {:else if tab === 'draw'}
            <div class="area">
                <Simulation
                    {api}
                    main={zones.main}
                    {categories}
                    {rules}
                    bind:result={simulation}
                    on:select={select}
                    on:categories={({ detail }) => {
                        categories = detail;
                        categoriesVersion += 1;
                    }}
                />
            </div>
        {:else if tab === 'history'}
            <div class="area">
                <History {api} bind:deck {zones} {dirty} />
            </div>
        {:else}
            <div class="area">
                <VersionPanel
                    {api}
                    bind:deck
                    bind:key
                    bind:dirty
                    on:reload={({ detail }) => setDeck(detail)}
                />
            </div>
        {/if}
    </div>
{:else}
    <p>Loading…</p>
{/if}

<Dialog />
<Toast />
