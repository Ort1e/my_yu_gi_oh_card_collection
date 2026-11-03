<script lang="ts">
    import { onMount } from 'svelte';
    import { deckApi } from './api';
    import { ZONES, euros, rulesFields, rulesKey, rulesOf, sortCards, zonePrice } from './deck';
    import type { ApiCard } from './api-types/ApiCard';
    import type { ApiCategory } from './api-types/ApiCategory';
    import type { Check } from './api-types/Check';
    import type { DeckVersion } from './api-types/DeckVersion';
    import type { Zone } from './api-types/Zone';
    import type { Move, Place, Zones } from './types';
    import CardOverview from './lib/CardOverview.svelte';
    import DeckZone from './lib/DeckZone.svelte';
    import ReserveSearch from './lib/ReserveSearch.svelte';
    import Simulation from './lib/Simulation.svelte';
    import Tournament from './lib/Tournament.svelte';
    import VersionPanel from './lib/VersionPanel.svelte';
    import YdkeToolbar from './lib/YdkeToolbar.svelte';

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
    let check: Check = { format: null, problems: [] };
    let dirty = false;
    let selected: ApiCard | null = null;
    let categories: ApiCategory[] = [];
    let categoriesVersion = 0;

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
            check = { format: null, problems: [`Could not check the format: ${e.message}`] };
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

            if (selected?.id === card.id) selected = null;
        } catch (e) {
            alert(`Could not delete the proxy: ${e.message}`);
        }
    }

    /** The deck zone (not the reserve) of `zones` holding `card`, or null. */
    const zoneOf = (zones: Zones, card: ApiCard | null): Zone | null =>
        card ? (ZONES.find((z) => zones[z].some((c) => c.id === card.id)) ?? null) : null;

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

            if (proxied_in.length) {
                alert(`Copy taken from ${proxied_in.join(', ')}: a proxy of it is there now.`);
            }
        } catch (e) {
            alert(`Could not replace the proxy: ${e.message}`);
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
                if (confirm(`Delete the proxy "${card.name}"?`)) await deleteProxy(from, card);

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

    async function addProxy({ detail: zone }: CustomEvent<Zone>) {
        const name = prompt('Enter proxy card name:');

        if (!name?.trim()) return;

        try {
            const card = await api.addProxy(name.trim(), zone);
            zones[zone] = sortCards([...zones[zone], card]);
        } catch (e) {
            alert(`Could not add the proxy: ${e.message}`);
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
                alert(['Deck saved! Now proxies in the other decks:', ...taken.map((l) => `• ${l}`)].join('\n'));
            } else {
                alert('Deck saved!');
            }
        } catch (e) {
            alert(`Save failed: ${e.message}`);
        }
    }

    function beforeUnload(e: BeforeUnloadEvent) {
        if (dirty) e.preventDefault();
    }

    const select = ({ detail }: CustomEvent<ApiCard>) => (selected = detail);

    $: total = zonePrice(zones.main) + zonePrice(zones.extra) + zonePrice(zones.side);
    $: title = deck ? `${deck.deck_name ?? 'No Deck'} - ${deck.version_name}` : '';
    $: deckCardIds = new Set(ZONES.flatMap((z) => zones[z].map((c) => c.id)));
</script>


<svelte:window on:dragover|preventDefault on:drop={dropOutside} on:beforeunload={beforeUnload} />

{#if loadError}
    <p class="error">Could not load the deck: {loadError}</p>
{:else if deck}
    <h2 class="title">
        Deck Builder: {title} ({euros(total)})
        {#if dirty}<span class="unsaved">● unsaved</span>{/if}
    </h2>

    <YdkeToolbar deckId={deck.deck_id} {zones} replaceable={isEmpty} removeVersion={api.remove} />

    <div class="builder-container">
        <div id="deck-area">
            {#each ZONE_LABELS as [zone, label]}
                <div class="area">
                    <h3>{label} ({zones[zone].length}, {euros(zonePrice(zones[zone]))})</h3>
                    <DeckZone
                        {zone}
                        cards={zones[zone]}
                        {rules}
                        selectedId={selected?.id}
                        on:move={({ detail }) => move(detail)}
                        on:proxy={addProxy}
                        on:select={select}
                    >
                        <span slot="empty" class="hint">Drop cards here, right-click to add a proxy.</span>
                    </DeckZone>
                </div>
            {/each}
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

        <div id="actions-area">
            <VersionPanel
                {api}
                {deckVersionId}
                bind:deck
                bind:key
                {check}
                bind:dirty
                on:save={save}
                on:reload={({ detail }) => setDeck(detail)}
            />
        </div>

        <div id="reserve-area" class="area">
            <h3>Reserve</h3>
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

        <div id="tournament-area" class="area">
            <h3>Tournament Info</h3>
            <Tournament {deckVersionId} tournament={deck.tournament} />
        </div>

        <div id="test-draw-area" class="area">
            <h3>Test Draw</h3>
            <Simulation
                {api}
                main={zones.main}
                {categories}
                {rules}
                {dirty}
                on:select={select}
                on:categories={({ detail }) => {
                    categories = detail;
                    categoriesVersion += 1;
                }}
            />
        </div>
    </div>
{:else}
    <p>Loading…</p>
{/if}
