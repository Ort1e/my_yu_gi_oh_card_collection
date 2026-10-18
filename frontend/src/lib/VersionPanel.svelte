<script>
    // The control panel: switch, rename, copy or delete the version, choose the
    // ban list, save. Saving needs the zones, so it is left to App.svelte
    // ("save" event).
    import { createEventDispatcher } from 'svelte';

    export let api;
    export let deckVersionId;
    /** the loaded version (shared with App.svelte: a rename updates the title) */
    export let deck;
    export let banListId;
    export let dirty;

    const dispatch = createEventDispatcher();

    function switchVersion(e) {
        location.href = `/deck-builder/${e.target.value}`;
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
</script>

<div class="control">
    <label for="version-select">Version</label>
    <select id="version-select" value={deckVersionId} on:change={switchVersion}>
        {#each deck.versions as version (version.id)}
            <option value={version.id}>{version.version_name}</option>
        {/each}
    </select>
    <div class="button-row">
        <button type="button" class="btn-secondary" on:click={rename}>Rename</button>
        <button type="button" class="btn-secondary" on:click={newVersion}>Copy as new version</button>
        <button type="button" class="btn-danger" on:click={remove}>Delete</button>
    </div>
</div>

<div class="control">
    <label for="ban-list-select">Ban list</label>
    <select id="ban-list-select" bind:value={banListId} on:change={() => (dirty = true)}>
        {#each deck.ban_lists as banList, i (banList.id)}
            <option value={banList.id}>{banList.date}{i === 0 ? ' (latest)' : ''}</option>
        {/each}
        <option value={null}>No ban list</option>
    </select>
    <span class="hint">Limits shown on the cards. Saved with the deck.</span>
</div>

<div class="control save-row">
    <button type="button" on:click={() => dispatch('save')}>Save the deck</button>
    {#if dirty}
        <span class="warning">Unsaved changes</span>
    {:else}
        <span class="hint">Everything is saved</span>
    {/if}
</div>

<style>
    .control select {
        max-width: none;
    }

    .button-row {
        margin: 0;
    }

    .save-row {
        display: flex;
        align-items: center;
        gap: 1rem;
        padding-top: 1rem;
        border-top: 1px solid rgba(255, 255, 255, 0.25);
    }
</style>
