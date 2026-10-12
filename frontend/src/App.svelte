<script>
    import { onMount } from 'svelte';
    import { deckApi, importYdke } from './api.js';
    import { ZONES, euros, sortCards, ydkeUrl, zonePrice } from './deck.js';
    import CardOverview from './lib/CardOverview.svelte';
    import DeckZone from './lib/DeckZone.svelte';
    import Simulation from './lib/Simulation.svelte';
    import Tournament from './lib/Tournament.svelte';

    export let deckVersionId;

    const api = deckApi(deckVersionId);

    let deck = null; // last loaded/saved version (metadata)
    let loadError = null;
    /** zone name -> cards; "reserve" holds the search results */
    let zones = { main: [], extra: [], side: [], reserve: [] };
    let banListId = null;
    let dirty = false;
    let selected = null;
    let categories = [];
    let categoriesVersion = 0;
    let copyOpen = false;
    let copied = '';

    let filters = { name: '', code: '', card_type: '' };
    let reserveMessage = 'Please enter at least one filter to search cards.';

    function setDeck(data) {
        deck = data;
        banListId = data.ban_list_id;
        zones = {
            main: sortCards(data.main),
            extra: sortCards(data.extra),
            side: sortCards(data.side),
            reserve: zones.reserve,
        };

        dirty = false;
    }

    onMount(async () => {
        try {
            setDeck(await api.get());
            categories = await api.categories();
        } catch (e) {
            loadError = e.message;
        }
    });

    // ---------- moving cards ----------

    function take(zone, cardId) {
        const card = zones[zone].find((c) => c.id === cardId);
        zones[zone] = zones[zone].filter((c) => c.id !== cardId);

        return card;
    }

    async function deleteProxy(from, card) {
        try {
            await api.deleteProxy(card.id);
            take(from, card.id);

            if (selected?.id === card.id) selected = null;
        } catch (e) {
            alert(`Could not delete the proxy: ${e.message}`);
        }
    }

    async function move({ cardId, from, to }) {
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

    function dropOutside(e) {
        e.preventDefault();
        const data = e.dataTransfer?.getData('application/json');

        if (data) move({ ...JSON.parse(data), to: 'outside' });
    }

    async function addProxy({ detail: zone }) {
        const name = prompt('Enter proxy card name:');

        if (!name?.trim()) return;

        try {
            const card = await api.addProxy(name.trim(), zone);
            zones[zone] = sortCards([...zones[zone], card]);
        } catch (e) {
            alert(`Could not add the proxy: ${e.message}`);
        }
    }

    // ---------- reserve search ----------

    let searchTimer;
    let searchId = 0;

    function scheduleSearch() {
        clearTimeout(searchTimer);
        searchTimer = setTimeout(search, 250);
    }

    async function search() {
        const active = filters.name.trim() || filters.code.trim() || filters.card_type;

        if (!active) {
            zones.reserve = [];
            reserveMessage = 'Please enter at least one filter to search cards.';

            return;
        }

        const id = ++searchId;

        try {
            const cards = await api.reserve(filters);

            if (id !== searchId) return; // a newer search started
            // cards moved into the deck but not saved yet are still in the results
            const inDeck = new Set(ZONES.flatMap((z) => zones[z].map((c) => c.id)));
            zones.reserve = sortCards(cards.filter((c) => !inDeck.has(c.id)));
            reserveMessage = 'No card found.';
        } catch (e) {
            reserveMessage = `Search failed: ${e.message}`;
        }
    }

    // ---------- actions ----------

    async function save() {
        try {
            setDeck(
                await api.update({
                    main: zones.main.map((c) => c.id),
                    extra: zones.extra.map((c) => c.id),
                    side: zones.side.map((c) => c.id),
                    ban_list_id: banListId,
                }),
            );
            alert('Deck saved!');
        } catch (e) {
            alert(`Save failed: ${e.message}`);
        }
    }

    async function rename() {
        const name = prompt('Enter new version name:', deck.version_name);

        if (!name?.trim()) return;

        try {
            const updated = await api.update({ version_name: name.trim() });
            deck.version_name = updated.version_name;
            deck.versions = updated.versions;
        } catch (e) {
            alert(`Rename failed: ${e.message}`);
        }
    }

    async function newVersion() {
        if (dirty && !confirm('The new version is a copy of the saved deck: unsaved changes are not included. Continue?')) return;

        const name = prompt('Enter name for the new version:', `${deck.version_name} (copy)`);

        if (!name?.trim()) return;

        try {
            const { id } = await api.clone(name.trim());
            dirty = false;
            location.href = `/deck-builder/${id}`;
        } catch (e) {
            alert(`Could not create the version: ${e.message}`);
        }
    }

    async function remove() {
        if (!confirm('Are you sure you want to delete this deck version?')) return;

        try {
            const { deck_id } = await api.remove();
            dirty = false;
            location.href = deck_id ? `/decks/${deck_id}` : '/decks';
        } catch (e) {
            alert(`Delete failed: ${e.message}`);
        }
    }

    async function doImport() {
        if (!deck.deck_id) return alert('This version has no deck to import into.');
        const ydke = prompt('Paste your YDKE string:');

        if (!ydke?.trim()) return;

        const name = prompt('Name of the new version:', 'Imported');

        if (name === null) return;

        try {
            const { id } = await importYdke(deck.deck_id, ydke.trim(), name);
            location.href = `/deck-builder/${id}`;
        } catch (e) {
            alert(`Import failed: ${e.message}`);
        }
    }

    const variants = {
        'With Proxies': () => true,
        'Without Proxies': (c) => !c.is_proxy,
        'Only Proxies': (c) => c.is_proxy,
    };

    async function copy(label) {
        copyOpen = false;

        try {
            await navigator.clipboard.writeText(ydkeUrl(zones, variants[label]));
            copied = label;
            setTimeout(() => (copied = ''), 2000);
        } catch (e) {
            alert(`Failed to copy: ${e.message}`);
        }
    }

    function switchVersion(e) {
        location.href = `/deck-builder/${e.target.value}`;
    }

    function beforeUnload(e) {
        if (dirty) e.preventDefault();
    }

    const select = ({ detail }) => (selected = detail);

    $: total = zonePrice(zones.main) + zonePrice(zones.extra) + zonePrice(zones.side);
    $: title = deck ? `${deck.deck_name ?? 'No Deck'} - ${deck.version_name}` : '';
</script>

<svelte:window on:dragover|preventDefault on:drop={dropOutside} on:beforeunload={beforeUnload} on:click={() => (copyOpen = false)} />

{#if loadError}
    <p class="error">Could not load the deck: {loadError}</p>
{:else if deck}
    <h2 class="title">
        (<button type="button" class="copy-btn" on:click={doImport}>Import YDKE</button>)
        Deck Builder : {title} ({euros(total)})
        <span class="copy-wrapper">
            (<button type="button" class="copy-btn" on:click|stopPropagation={() => (copyOpen = !copyOpen)}>
                {copied ? `Copied ${copied}!` : 'Copy YDKE'}
            </button>)
            {#if copyOpen}
                <div class="copy-popup">
                    {#each Object.keys(variants) as label}
                        <button type="button" on:click|stopPropagation={() => copy(label)}>{label}</button>
                    {/each}
                </div>
            {/if}
        </span>
        {#if dirty}<span class="unsaved">● unsaved</span>{/if}
    </h2>

    <div class="builder-container">
        <div id="deck-area">
            {#each [['main', 'Main Deck'], ['extra', 'Extra Deck'], ['side', 'Side Deck']] as [zone, label]}
                <div class="area">
                    <h3>{label} ({zones[zone].length}, {euros(zonePrice(zones[zone]))})</h3>
                    <DeckZone
                        {zone}
                        cards={zones[zone]}
                        {banListId}
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
            <CardOverview card={selected} {banListId} {api} {categoriesVersion} />
        </div>

        <div id="actions-area" class="actions">
            <button type="button" on:click={save}>💾 Save</button>
            <button type="button" on:click={rename}>✏️ Rename</button>
            <button type="button" on:click={newVersion}>📦 New</button>
            <button type="button" on:click={remove}>🗑️ Delete</button>

            <select bind:value={banListId} on:change={() => (dirty = true)} title="Ban list">
                {#each deck.ban_lists as banList (banList.id)}
                    <option value={banList.id}>{banList.date}</option>
                {/each}
                <option value={null}>No Ban List</option>
            </select>

            <select value={deckVersionId} on:change={switchVersion} title="Version">
                {#each deck.versions as version (version.id)}
                    <option value={version.id}>{version.version_name}</option>
                {/each}
            </select>
        </div>

        <div id="reserve-area" class="area">
            <h3>Reserve</h3>
            <input type="text" class="deck-builder-input" placeholder="Search cards..." bind:value={filters.name} on:input={scheduleSearch} />
            <input type="text" class="deck-builder-input" placeholder="Search by code..." bind:value={filters.code} on:input={scheduleSearch} />
            <select class="deck-builder-input" bind:value={filters.card_type} on:change={scheduleSearch}>
                <option value="">Card Type (any)</option>
                {#each deck.card_types as cardType}
                    <option value={cardType}>{cardType}</option>
                {/each}
            </select>
            <div id="reserve">
                <DeckZone
                    zone="reserve"
                    cards={zones.reserve}
                    {banListId}
                    selectedId={selected?.id}
                    on:move={({ detail }) => move(detail)}
                    on:select={select}
                >
                    <span slot="empty" class="hint">{reserveMessage}</span>
                </DeckZone>
            </div>
        </div>

        <div id="tournament-area" class="area">
            <h3>Tournament Info</h3>
            <Tournament {api} tournament={deck.tournament} />
        </div>

        <div id="test-draw-area" class="area">
            <h3>Test Draw</h3>
            <Simulation
                {api}
                main={zones.main}
                {categories}
                {banListId}
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
