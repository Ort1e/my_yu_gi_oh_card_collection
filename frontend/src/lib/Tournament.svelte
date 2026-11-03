<script lang="ts">
    // The tournament this version was played at: a summary, edited on the
    // tournament pages (/tournaments, crates/yugioh/src/routes/tournaments/).
    import { frenchDate, ordinal } from '../deck';
    import type { Tournament } from '../api-types/Tournament';

    export let deckVersionId: number;
    export let tournament: Tournament | null = null;

    $: placement = !tournament?.placement
        ? ''
        : tournament.players
          ? `${ordinal(tournament.placement)} of ${tournament.players}`
          : ordinal(tournament.placement);
</script>

{#if tournament}
    <p class="tournament-summary">
        <a href="/tournaments/{tournament.id}" target="_blank" rel="noopener">{tournament.name}</a>,
        {frenchDate(tournament.date)}{#if tournament.location}, {tournament.location}{/if}
        {#if placement}<br />Placement: {placement}{/if}
    </p>

    <div class="button-row">
        <a class="btn btn-small" href="/tournaments/{tournament.id}" target="_blank" rel="noopener">Rounds</a>
        <a class="btn btn-small btn-secondary" href="/tournaments/{tournament.id}/edit" target="_blank" rel="noopener">Edit</a>
    </div>
{:else}
    <p class="hint">Not played in a tournament yet.</p>

    <a class="btn btn-small" href="/tournaments/add?version={deckVersionId}" target="_blank" rel="noopener">Add a tournament</a>
{/if}

<style>
    .tournament-summary {
        margin: 0 0 0.5rem;
    }
</style>
