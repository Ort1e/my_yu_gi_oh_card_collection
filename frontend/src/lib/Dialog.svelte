<script lang="ts">
    // The one dialog of the builder, a native <dialog> opened for whatever
    // dialog.ts asks (a question, a text, a report).
    import { tick } from 'svelte';

    import { current, done } from './dialog';

    let dialog: HTMLDialogElement;
    let field: HTMLInputElement | HTMLTextAreaElement | undefined;
    let text = '';

    $: request = $current;

    $: if (request) open(request);

    async function open(shown: NonNullable<typeof request>) {
        text = shown.kind === 'ask' ? (shown.options.default ?? '') : '';

        await tick();

        if (!dialog.open) dialog.showModal();

        field?.focus();
        field?.select();
    }

    function answer(value: string | boolean | null) {
        const shown = request;

        if (!shown) return;

        if (dialog.open) dialog.close();

        if (shown.kind === 'ask') shown.resolve(value);
        else shown.resolve();

        done();
    }

    const confirmed = () => answer(request?.kind === 'ask' && request.options.input ? text : true);

    // Escape and a click on the backdrop cancel
    const cancelled = () => answer(request?.kind === 'ask' && request.options.input ? null : false);

    function backdrop(e: MouseEvent) {
        if (e.target === dialog) cancelled();
    }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
<dialog bind:this={dialog} class="builder-dialog" on:cancel|preventDefault={cancelled} on:click={backdrop}>
    {#if request}
        <form on:submit|preventDefault={confirmed}>
            <h3>{request.options.title}</h3>

            {#if request.kind === 'ask'}
                {#if request.options.message}<p class="message">{request.options.message}</p>{/if}

                {#if request.options.input === 'multiline'}
                    <textarea bind:this={field} bind:value={text} rows="5" placeholder={request.options.placeholder ?? ''}></textarea>
                {:else if request.options.input}
                    <input bind:this={field} type="text" bind:value={text} placeholder={request.options.placeholder ?? ''} />
                {/if}

                <div class="buttons">
                    <button type="button" class="btn-secondary" on:click={cancelled}>Cancel</button>
                    <button type="submit" class:btn-danger={request.options.danger}>
                        {request.options.confirmLabel ?? 'OK'}
                    </button>
                </div>
            {:else}
                {#each request.options.lines as line}
                    <p class="line">{line}</p>
                {/each}

                <div class="buttons">
                    <button type="submit">OK</button>
                </div>
            {/if}
        </form>
    {/if}
</dialog>

<style>
    .builder-dialog {
        width: min(32rem, 92vw);
        padding: 1.2rem;
        border: none;
        border-radius: 12px;
        background: #fff;
        color: #222;
        box-shadow: 0 8px 30px rgba(0, 0, 0, 0.4);
    }

    .builder-dialog::backdrop {
        background: rgba(0, 0, 0, 0.5);
    }

    h3 {
        margin: 0 0 0.6rem;
        color: var(--primary);
    }

    .message,
    .line {
        margin: 0 0 0.5rem;
        white-space: pre-line;
    }

    input,
    textarea {
        width: 100%;
        max-width: none;
        box-sizing: border-box;
    }

    .buttons {
        display: flex;
        justify-content: flex-end;
        gap: 0.5rem;
        margin-top: 1rem;
    }
</style>
