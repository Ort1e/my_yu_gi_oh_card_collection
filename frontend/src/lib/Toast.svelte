<script lang="ts">
    import { dismiss, messages } from './toast';
</script>

<div class="toasts" aria-live="polite">
    {#each $messages as message (message.id)}
        <button type="button" class="toast" class:error={message.error} on:click={() => dismiss(message.id)}>
            {message.text}
        </button>
    {/each}
</div>

<style>
    .toasts {
        position: fixed;
        right: 1rem;
        bottom: 1rem;
        z-index: 100;
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: 0.4rem;
        max-width: min(28rem, 90vw);
    }

    .toast {
        padding: 0.5rem 0.9rem;
        border-radius: 8px;
        background: #2b7a3d;
        color: #fff;
        font-weight: 600;
        text-align: left;
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
        cursor: pointer;
    }

    .toast.error {
        background: var(--danger, #b3261e);
    }
</style>
