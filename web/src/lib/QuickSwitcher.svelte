<script lang="ts">
  import { onMount } from 'svelte';
  import { getMruList } from './panelHistory.svelte';
  import { focusPanel, PANEL_DEFS } from './dockviewActions.svelte';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let selectedIndex = $state(0);
  let listEl: HTMLUListElement | undefined = $state();

  /** Build display list from MRU, enriched with panel titles. */
  const panelTitleMap: Record<string, string> = {};
  for (const def of PANEL_DEFS) {
    panelTitleMap[def.id] = def.title;
  }

  function getTitle(id: string): string {
    if (id === 'outliner' || id.startsWith('outliner-') || id.startsWith('ol-')) {
      return 'Outliner';
    }
    return panelTitleMap[id] ?? id;
  }

  let items = $derived(
    getMruList().map(id => ({ id, title: getTitle(id) }))
  );

  function commit() {
    const item = items[selectedIndex];
    if (item) focusPanel(item.id);
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown':
      case 'ArrowRight':
        e.preventDefault();
        if (items.length > 0) {
          selectedIndex = (selectedIndex + 1) % items.length;
        }
        break;
      case 'ArrowUp':
      case 'ArrowLeft':
        e.preventDefault();
        if (items.length > 0) {
          selectedIndex = (selectedIndex - 1 + items.length) % items.length;
        }
        break;
      case 'k':
        // Ctrl+K cycles forward (like repeated trigger)
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          if (items.length > 0) {
            selectedIndex = (selectedIndex + 1) % items.length;
          }
        }
        break;
      case 'Enter':
        e.preventDefault();
        commit();
        break;
      case 'Escape':
        e.preventDefault();
        onClose();
        break;
    }
  }

  onMount(() => {
    // Start with index 1 (skip the current panel, which is at 0)
    if (items.length > 1) selectedIndex = 1;

    // Focus the list so keyboard events work
    listEl?.focus();
  });

  $effect(() => {
    // Scroll selected item into view
    if (!listEl) return;
    const el = listEl.children[selectedIndex] as HTMLElement | undefined;
    el?.scrollIntoView({ block: 'nearest' });
  });
</script>

<div
  class="quick-switcher-backdrop"
  role="presentation"
  onkeydown={handleKeydown}
  onclick={onClose}
>
  <div
    class="quick-switcher"
    role="dialog"
    aria-label="Quick panel switcher"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <div class="qs-header">Switch Panel</div>
    {#if items.length === 0}
      <div class="qs-empty">No panels in history</div>
    {:else}
      <ul
        class="qs-list"
        role="listbox"
        aria-label="Panels"
        bind:this={listEl}
        tabindex="-1"
      >
        {#each items as item, i (item.id)}
          <li
            class="qs-item"
            class:selected={i === selectedIndex}
            role="option"
            aria-selected={i === selectedIndex}
            onclick={() => { selectedIndex = i; commit(); }}
            onkeydown={(e) => { if (e.key === 'Enter') { selectedIndex = i; commit(); } }}
          >
            <span class="qs-title">{item.title}</span>
            {#if i === 0}
              <span class="qs-badge">current</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <div class="qs-footer">
      <kbd>Arrow keys</kbd> or <kbd>Ctrl+K</kbd> to navigate &middot;
      <kbd>Enter</kbd> to switch &middot;
      <kbd>Esc</kbd> to cancel
    </div>
  </div>
</div>

<style>
  .quick-switcher-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9500;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 15vh;
    background: rgba(0, 0, 0, 0.4);
  }

  .quick-switcher {
    width: 400px;
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }

  .qs-header {
    padding: 12px 16px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-color);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .qs-empty {
    padding: 24px 16px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  .qs-list {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    overflow-y: auto;
    outline: none;
  }

  .qs-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    cursor: pointer;
    color: var(--text-secondary);
    font-size: 14px;
  }

  .qs-item:hover {
    background: var(--bg-tertiary);
  }

  .qs-item.selected {
    background: var(--accent-color);
    color: var(--bg-primary);
  }

  .qs-title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .qs-badge {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--bg-tertiary);
    color: var(--text-muted);
    margin-left: 8px;
  }

  .qs-item.selected .qs-badge {
    background: rgba(255, 255, 255, 0.2);
    color: var(--bg-primary);
  }

  .qs-footer {
    padding: 8px 16px;
    font-size: 11px;
    color: var(--text-muted);
    border-top: 1px solid var(--border-color);
    text-align: center;
  }

  .qs-footer kbd {
    display: inline-block;
    padding: 1px 4px;
    border: 1px solid var(--border-color);
    border-radius: 3px;
    font-family: inherit;
    font-size: 10px;
    background: var(--bg-primary);
    color: var(--text-secondary);
  }
</style>
