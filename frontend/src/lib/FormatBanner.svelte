<script lang="ts">
    // Above the zones: whether the saved deck fits its format (the problems
    // unfold), and the last test draw's headline.
    import type { Check } from '../api-types/Check';
    import type { ApiCategory } from '../api-types/ApiCategory';
    import type { SimulationResult } from '../api-types/SimulationResult';

    export let check: Check;
    export let dirty: boolean;
    export let result: SimulationResult | null;
    export let categories: ApiCategory[];

    let open = false;

    $: problems = check.problems.length + check.errata.length;

    $: hits = (result?.categories ?? []).map((s) => {
        const none = s.per_hand[0] ?? 0;

        return { name: s.name, percent: Math.round(((result!.total_simulations - none) * 100) / result!.total_simulations) };
    });

    $: names = new Set(categories.map((c) => c.name));
</script>

<div class="banner">
    {#if check.format && problems === 0}
        <span class="ok">✓ Fits {check.format}</span>
    {:else if problems > 0}
        <button type="button" class="problems" on:click={() => (open = !open)}>
            {problems} problem{problems === 1 ? '' : 's'}{check.format ? ` in ${check.format}` : ''} {open ? '▾' : '▸'}
        </button>
    {:else}
        <span class="hint">No format chosen (Settings tab).</span>
    {/if}

    {#if dirty}<span class="hint">(as saved)</span>{/if}

    {#if hits.length > 0}
        <a class="hits" href="#draw" title="Last test draw: hands with at least one card of each category">
            Test draw:
            {#each hits.filter((h) => names.has(h.name)) as hit}<span class="hit">{hit.name} {hit.percent}%</span>{/each}
        </a>
    {/if}
</div>

{#if open}
    <div class="details">
        {#if check.problems.length > 0}
            <ul>
                {#each check.problems as problem}<li>{problem}</li>{/each}
            </ul>
        {/if}

        {#if check.errata.length > 0}
            <p class="title">⚠ Text changed since the cutoff:</p>
            <ul>
                {#each check.errata as card}
                    <li>
                        {card.count} × {card.name}
                        {#if card.warning}: buy {card.warning.right_sets}{/if}
                    </li>
                {/each}
            </ul>
            <p class="hint">More in the Missing tab.</p>
        {/if}
    </div>
{/if}

<style>
    .banner {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 0.4rem 1rem;
        margin-bottom: 0.5rem;
    }

    .ok {
        color: #8be28b;
        font-weight: bold;
    }

    .problems {
        padding: 0.15rem 0.6rem;
        border-radius: 999px;
        background: var(--danger);
        color: #fff;
        font-size: 0.9rem;
    }

    .hits {
        margin-left: auto;
        color: var(--text-light);
        font-size: 0.9rem;
    }

    .hit {
        margin-left: 0.5rem;
        padding: 0.05rem 0.4rem;
        border-radius: 4px;
        background: rgba(0, 0, 0, 0.3);
    }

    .details {
        margin-bottom: 0.6rem;
        padding: 0.4rem 0.8rem;
        border-radius: 8px;
        background: rgba(0, 0, 0, 0.25);
        color: #ffb3b3;
    }

    .details ul {
        margin: 0;
        padding-left: 1.2rem;
    }

    .title {
        margin: 0.4rem 0 0.2rem;
        font-weight: bold;
    }
</style>
