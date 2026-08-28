<script>
  import { onMount } from 'svelte';

  let items = [];
  let newName = '';

  async function loadItems() {
    const res = await fetch('/api/items');
    items = await res.json();
  }

  async function addItem() {
    if (!newName.trim()) return;
    await fetch('/api/items', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newName }),
    });
    newName = '';
    await loadItems();
  }

  onMount(loadItems);
</script>

<h1>Complex page (Svelte)</h1>

<input bind:value={newName} placeholder="New item" />
<button on:click={addItem}>Add</button>

<ul>
  {#each items as item (item.id)}
    <li>{item.name}</li>
  {/each}
</ul>

<style>
  h1 { font-family: sans-serif; }
</style>
