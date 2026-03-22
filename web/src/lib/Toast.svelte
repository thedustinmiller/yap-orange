<script lang="ts">
  import { getToasts, removeToast } from './toastStore.svelte';

  let toasts = $derived(getToasts());
</script>

{#if toasts.length > 0}
  <div class="toast-container" role="status" aria-live="polite">
    {#each toasts as toast (toast.id)}
      <div class="toast toast-{toast.type}" role="alert">
        <span class="toast-icon">
          {#if toast.type === 'error'}&#x26A0;{:else if toast.type === 'warning'}&#x26A0;{:else}&#x2139;{/if}
        </span>
        <span class="toast-message">{toast.message}</span>
        <button class="toast-close" aria-label="Dismiss notification" onclick={() => removeToast(toast.id)}>&times;</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-container {
    position: fixed;
    bottom: 16px;
    right: 16px;
    z-index: 9999;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 400px;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-radius: 6px;
    font-size: 13px;
    color: #fff;
    pointer-events: auto;
    animation: toast-in 0.2s ease-out;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  .toast-error {
    background: #e06c75;
  }

  .toast-warning {
    background: #d19a66;
  }

  .toast-info {
    background: var(--accent-color, #7aa2f7);
  }

  .toast-icon {
    flex-shrink: 0;
    font-size: 14px;
  }

  .toast-message {
    flex: 1;
    line-height: 1.4;
  }

  .toast-close {
    flex-shrink: 0;
    font-size: 16px;
    opacity: 0.7;
    transition: opacity 0.1s;
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    padding: 0;
    line-height: 1;
  }

  .toast-close:hover {
    opacity: 1;
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
