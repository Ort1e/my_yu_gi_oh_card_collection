<script>
    // Adds a category to the deck version: a plain one, or a conditional one
    // (a card that counts as one category or another depending on the rest
    // of the hand, see ApiCondition in backend/src/routes/decks/api/categories.rs).
    import { createEventDispatcher } from 'svelte';

    export let api;
    export let categories;

    const dispatch = createEventDispatcher();

    let name = '';
    let conditional = false;
    let description = '';
    let ifAnyOf = [];
    let then = null;
    let otherwise = null;

    function reset() {
        name = '';
        conditional = false;
        description = '';
        ifAnyOf = [];
        then = null;
        otherwise = null;
    }

    async function add() {
        if (!name.trim()) return;

        let condition = null;

        if (conditional) {
            if (ifAnyOf.length === 0) return alert('Choose at least one category for the condition.');

            if (then === null) return alert('Choose the category it counts as when the condition is met.');

            condition = { description, if_any_of: ifAnyOf, then, otherwise };
        }

        try {
            await api.addCategory(name.trim(), condition);
            reset();
            dispatch('added');
        } catch (e) {
            alert(`Could not add the category: ${e.message}`);
        }
    }
</script>

<form class="plain category-form" on:submit|preventDefault={add}>
    <h4>New category</h4>

    <div class="field-row">
        <div class="wide">
            <label for="category-name">Name</label>
            <input id="category-name" type="text" bind:value={name} maxlength="255" placeholder="e.g. starter" />
        </div>
        <label class="switch"><input type="checkbox" bind:checked={conditional} /> Conditional</label>
    </div>

    {#if conditional}
        <p class="hint">
            A card of a conditional category counts as one category when the hand also has a card of some
            other categories, and as another one (or nothing) otherwise.
        </p>

        <label for="category-description">Condition, in your words</label>
        <input
            id="category-description"
            type="text"
            bind:value={description}
            required
            placeholder="e.g. only a starter with another name or an extender"
        />

        <span class="label">The hand also has a card of one of:</span>
        <div class="choices">
            {#each categories as category (category.id)}
                <label><input type="checkbox" value={category.id} bind:group={ifAnyOf} /> {category.name}</label>
            {:else}
                <span class="hint">Create the other categories first.</span>
            {/each}
        </div>

        <div class="field-row">
            <div>
                <label for="category-then">Then it counts as</label>
                <select id="category-then" bind:value={then} required>
                    <option value={null} disabled>Choose a category</option>
                    {#each categories as category (category.id)}
                        <option value={category.id}>{category.name}</option>
                    {/each}
                </select>
            </div>
            <div>
                <label for="category-otherwise">Otherwise it counts as</label>
                <select id="category-otherwise" bind:value={otherwise}>
                    <option value={null}>Nothing</option>
                    {#each categories as category (category.id)}
                        <option value={category.id}>{category.name}</option>
                    {/each}
                </select>
            </div>
        </div>
    {/if}

    <button type="submit">Add the category</button>
</form>

<style>
    /* The "new category" form, under the statistics. */
    .category-form {
        margin-top: 1rem;
        padding-top: 0.5rem;
        border-top: 1px solid rgba(255, 255, 255, 0.25);
    }

    .category-form h4 {
        margin: 0.5rem 0;
    }
</style>
