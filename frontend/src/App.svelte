<script lang="ts">
    import { onMount } from 'svelte';
    import { deckApi, importYdke } from './api';
    import { ZONES, euros, frenchDate, rulesFields, rulesKey, rulesOf, sortCards, zonePrice } from './deck';
    import type { ApiCard } from './api-types/ApiCard';
    import type { ApiCategory } from './api-types/ApiCategory';
    import type { Check } from './api-types/Check';
    import type { DeckVersion } from './api-types/DeckVersion';
    import type { SimulationResult } from './api-types/SimulationResult';
    import type { Zone } from './api-types/Zone';
    import type { Move, Place, Zones } from './types';
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
    import { askText, askYes, inform } from './lib/dialog';
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
    let check: Check = { format: null, problems: [], errata: [], errata_pending: 0, wants: '' };
    let dirty = false;
    let selected: ApiCard | null = null;
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
            main: sortCards(data.main),
            extra: sortCards(data.extra),
            side: sortCards(data.side),
            reserve: zones.reserve,
        };

        dirty = false;
        refreshCheck();
    }

    async function refreshCheck() {
        try {
            check = await api.check();
        } catch (e) {
            check = { format: null, problems: [`Could not check the format: ${e.message}`], errata: [], errata_pending: 0, wants: '' };
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

    function take(zone: Place, cardId: number) {
        const card = zones[zone].find((c) => c.id === cardId);
        zones[zone] = zones[zone].filter((c) => c.id !== cardId);

        return card;
    }

    async function deleteProxy(from: Place, card: ApiCard) {
        try {
            await api.deleteProxy(card.id);
            take(from, card.id);
            refreshCheck();

            if (selected?.id === card.id) selected = null;
        } catch (e) {
            toast.error(`Could not delete the proxy: ${e.message}`);
        }
    }

    /** The deck zone (not the reserve) of `zones` holding `card`, or null. */
    const zoneOf = (zones: Zones, card: ApiCard | null): Zone | null =>
        card ? (['main', 'extra', 'side'] as Zone[]).find((z) => zones[z].some((c) => c.id === card.id)) ?? null : null;

    $: selectedZone = zoneOf(zones, selected);

    // a proxy swapped for an owned copy (saved at once; the only way in a
    // built version, whose proxies are never swapped automatically)
    async function replaceProxy({ detail: proxy }: CustomEvent<ApiCard>) {
        const zone = zoneOf(zones, proxy);

        if (!zone) return;

        try {
            const { card, proxied_in } = await api.replaceProxy(proxy.id);

            zones[zone] = sortCards(zones[zone].map((c) => (c.id === proxy.id ? card : c)));
            zones.reserve = zones.reserve.filter((c) => c.id !== card.id);
            selected = card;
            categoriesVersion += 1;
            refreshCheck();

            if (proxied_in.length) {
                await inform({
                    title: 'Copy taken from other decks',
                    lines: [`Copy taken from ${proxied_in.join(', ')}: a proxy of it is there now.`],
                });
            }
        } catch (e) {
            toast.error(`Could not replace the proxy: ${e.message}`);
        }
    }

    async function move({ cardId, from, to }: Move) {
        if (from === to || !zones[from]) return;

        const card = zones[from].find((c) => c.id === cardId);

        if (!card) return;

        if (to === 'reserve' || to === 'outside') {
            if (from === 'reserve') return;

            if (card.is_proxy) {
                // a proxy dragged out of the deck is deleted
                if (await askYes({ title: 'Delete the proxy?', message: `Delete the proxy "${card.name}"?`, confirmLabel: 'Delete', danger: true })) {
                    await deleteProxy(from, card);
                }

                return;
            }

            if (to === 'outside') return;
        }

        take(from, cardId);
        zones[to] = sortCards([...zones[to], card]);

        dirty = true;
    }

    function dropOutside(e: DragEvent) {
        e.preventDefault();
        const data = e.dataTransfer?.getData('application/json');

        if (data) move({ ...JSON.parse(data), to: 'outside' });
    }

    async function askProxy(zone: Zone) {
        const name = await askText({ title: 'Add a missing card', message: 'Name of the card, as a proxy:', confirmLabel: 'Add' });

        if (name) await addProxy(name, zone);
    }

    async function addProxy(name: string, zone: Zone) {
        try {
            const card = await api.addProxy(name, zone);

            zones[zone] = sortCards([...zones[zone], card]);
            refreshCheck();
            toast.ok(`${card.name} added as a proxy`);
        } catch (e) {
            toast.error(`Could not add the proxy: ${e.message}`);
        }
    }

    // ---------- saving ----------

    async function save() {
        try {
            const saved = await api.update({
                main: zones.main.map((c) => c.id),
                extra: zones.extra.map((c) => c.id),
                side: zones.side.map((c) => c.id),
                ...rulesFields(key),
            });

            setDeck(saved);

            // a built deck takes its new cards from the other decks
            const taken = saved.build_report?.proxied_elsewhere ?? [];

            if (taken.length > 0) {
                await inform({
                    title: 'Deck saved',
                    lines: ['Now proxies in the other decks:', ...taken.map((l) => `• ${l}`)],
                });
            } else {
                toast.ok('Deck saved');
            }
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
            message: 'The cards become this version (cards you own are used, the others are proxies).',
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

    const select = ({ detail }: CustomEvent<ApiCard>) => (selected = detail);

    $: total = zonePrice(zones.main) + zonePrice(zones.extra) + zonePrice(zones.side);
    $: rulesLabel = deck ? rulesLabelOf(deck, key) : null;

    function rulesLabelOf(deck: DeckVersion, key: string | null): string | null {
        if (key == null) return null;

        if (key.startsWith('f')) return deck.formats.find((f) => `f${f.id}` === key)?.label ?? null;

        const list = deck.ban_lists.find((b) => String(b.id) === key);

        return list ? `List of ${frenchDate(list.date)}` : null;
    }
    $: missingCount = zones.main.concat(zones.extra, zones.side).filter((c) => c.is_proxy).length;
    $: deckCardIds = new Set(ZONES.flatMap((z) => zones[z].map((c) => c.id)));
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
                            <p>Drop cards from your collection, or paste a YDKE.</p>
                            <button type="button" on:click={importIntoEmpty} disabled={deck.deck_id == null}>Paste a YDKE</button>
                        </div>
                    {/if}

                    {#each ZONE_LABELS as [zone, label]}
                        <div class="area">
                            <div class="zone-head">
                                <h3>{label} ({zones[zone].length}, {euros(zonePrice(zones[zone]))})</h3>
                                <button type="button" class="btn-small btn-secondary" on:click={() => askProxy(zone)}>+ Missing card</button>
                            </div>
                            <DeckZone
                                {zone}
                                cards={zones[zone]}
                                {rules}
                                selectedId={selected?.id}
                                on:move={({ detail }) => move(detail)}
                                on:proxy={({ detail }) => askProxy(detail)}
                                on:select={select}
                            >
                                <span slot="empty" class="hint">Drop cards here, right-click to add a missing card.</span>
                            </DeckZone>
                        </div>
                    {/each}
                </div>

                <aside class="side">
                    <div class="area">
                        <h3>From my collection</h3>
                        <ReserveSearch
                            {api}
                            cardTypes={deck.card_types}
                            bind:cards={zones.reserve}
                            {deckCardIds}
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
                            canReplace={selectedZone !== null}
                            on:replaceProxy={replaceProxy}
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
                    on:replace={replaceProxy}
                    on:add={({ detail }) => addProxy(detail.name, detail.zone)}
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
