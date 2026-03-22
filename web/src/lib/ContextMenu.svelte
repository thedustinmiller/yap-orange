<script lang="ts">
  import { tick } from 'svelte';
  import type { TreeNode } from './blockTree.svelte';
  import type { MenuAction, MenuContext } from './contextMenuActions';

  let {
    x,
    y,
    node,
    actions,
    ctx,
    onclose,
  }: {
    x: number;
    y: number;
    node: TreeNode;
    actions: MenuAction[];
    ctx: MenuContext;
    onclose: () => void;
  } = $props();

  let menuEl: HTMLDivElement | undefined = $state();
  let focusedIndex = $state(-1);

  // Focus the menu on mount for keyboard nav
  $effect(() => {
    if (menuEl) {
      tick().then(() => menuEl?.focus());
    }
  });

  // Clamp menu position to viewport
  let adjustedX = $derived.by(() => {
    if (!menuEl) return x;
    const w = menuEl.offsetWidth || 180;
    return Math.min(x, window.innerWidth - w - 8);
  });

  let adjustedY = $derived.by(() => {
    if (!menuEl) return y;
    const h = menuEl.offsetHeight || 200;
    return Math.min(y, window.innerHeight - h - 8);
  });

  function handleKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        focusedIndex = (focusedIndex + 1) % actions.length;
        break;
      case 'ArrowUp':
        e.preventDefault();
        focusedIndex = (focusedIndex - 1 + actions.length) % actions.length;
        break;
      case 'Enter':
        e.preventDefault();
        if (focusedIndex >= 0 && focusedIndex < actions.length) {
          executeAction(actions[focusedIndex]);
        }
        break;
      case 'Escape':
        e.preventDefault();
        onclose();
        break;
    }
  }

  function executeAction(action: MenuAction) {
    onclose();
    action.handler(node, ctx);
  }

  function handleBackdropContextMenu(e: MouseEvent) {
    e.preventDefault();
    onclose();
  }

  function stopProp(e: MouseEvent) {
    e.stopPropagation();
  }

  function stopPropContext(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<!-- Backdrop to catch clicks outside -->
<div class="context-menu-backdrop" role="presentation" onclick={onclose} oncontextmenu={handleBackdropContextMenu}>
  <div
    class="context-menu"
    role="menu"
    aria-label="Context menu"
    bind:this={menuEl}
    style="left: {adjustedX}px; top: {adjustedY}px"
    onclick={stopProp}
    oncontextmenu={stopPropContext}
    onkeydown={handleKeydown}
    tabindex="0"
  >
    {#each actions as action, i}
      <button
        class="context-menu-item"
        role="menuitem"
        class:focused={i === focusedIndex}
        tabindex={i === focusedIndex ? 0 : -1}
        onclick={() => executeAction(action)}
        onmouseenter={() => { focusedIndex = i; }}
      >
        {#if action.icon}
          <span class="item-icon">{action.icon}</span>
        {/if}
        <span class="item-label">{action.label}</span>
      </button>
      {#if action.dividerAfter}
        <div class="context-menu-divider"></div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .context-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9998;
  }

  .context-menu {
    position: fixed;
    z-index: 9999;
    min-width: 160px;
    max-width: 240px;
    background: var(--bg-secondary, #1e1e2e);
    border: 1px solid var(--border-color, #333);
    border-radius: 6px;
    padding: 4px 0;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
    outline: none;
    font-size: 13px;
  }

  .context-menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    cursor: pointer;
    color: var(--text-primary, #cdd6f4);
    transition: background 0.08s;
    background: none;
    border: none;
    font: inherit;
    width: 100%;
    text-align: left;
  }

  .context-menu-item:hover,
  .context-menu-item.focused {
    background: var(--bg-hover, rgba(255, 255, 255, 0.06));
  }

  .item-icon {
    width: 16px;
    text-align: center;
    flex-shrink: 0;
    font-size: 12px;
  }

  .item-label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .context-menu-divider {
    height: 1px;
    background: var(--border-color, #333);
    margin: 4px 8px;
  }
</style>
