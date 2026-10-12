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

<form on:submit|preventDefault={save}>
    <fieldset>
        <p><label>Name: <input type="text" bind:value={form.name} maxlength="255" required /></label></p>
        <p><label>Date: <input type="date" bind:value={form.date} required /></label></p>
        <p><label>Location: <input type="text" bind:value={form.location} maxlength="255" /></label></p>
        <p><label>Notes: <textarea bind:value={form.notes} rows="3"></textarea></label></p>
    </fieldset>
    <input type="submit" value="Save" /> {message}
</form>
