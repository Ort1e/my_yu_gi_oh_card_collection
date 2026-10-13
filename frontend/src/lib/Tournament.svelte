<script>
    export let api;
    export let tournament = null;

    let form = {
        name: tournament?.name ?? '',
        date: tournament?.date ?? '',
        location: tournament?.location ?? '',
        notes: tournament?.notes ?? '',
    };

    let message = '';

    async function save() {
        message = 'Saving…';

        try {
            tournament = await api.saveTournament(form);
            message = 'Saved.';
        } catch (e) {
            message = `Error: ${e.message}`;
        }
    }
</script>

<form class="plain tournament-form" on:submit|preventDefault={save}>
    <div class="field-row">
        <div class="wide">
            <label for="tournament-name">Name</label>
            <input id="tournament-name" type="text" bind:value={form.name} maxlength="255" required />
        </div>
        <div>
            <label for="tournament-date">Date</label>
            <input id="tournament-date" type="date" bind:value={form.date} required />
        </div>
        <div class="wide">
            <label for="tournament-location">Location</label>
            <input id="tournament-location" type="text" bind:value={form.location} maxlength="255" />
        </div>
    </div>

    <label for="tournament-notes">Notes</label>
    <textarea id="tournament-notes" bind:value={form.notes} rows="2"></textarea>

    <div class="button-row">
        <button type="submit">Save the tournament</button>
        {#if message}<span class="hint">{message}</span>{/if}
    </div>
</form>
