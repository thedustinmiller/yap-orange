<script lang="ts">
  import type { DockviewGroupPanel, DockviewHeaderPosition } from 'dockview-core';
  import {
    PANEL_DEFS,
    getDockviewRef,
    addPanel,
    removePanel,
    saveLayout,
  } from './dockviewActions.svelte';
  import { getSetting } from './settingsStore.svelte';

  interface Props {
    group: DockviewGroupPanel;
  }

  let { group }: Props = $props();
  let open = $state(false);
  let buttonEl: HTMLButtonElement | undefined = $state();

  // Snapshot panel state each time the menu opens
  let visibleIds = $state(new Set<string>());
  let currentPosition = $state<DockviewHeaderPosition>('top');
  let dropdownStyle = $state('');

  function toggleMenu() {
    if (!open) {
      // Refresh state on open
      const dv = getDockviewRef();
      if (dv) {
        visibleIds = new Set(dv.panels.map(p => p.id));
      }
      currentPosition = group.api.getHeaderPosition();

      // Position dropdown using fixed coords from button rect.
      // After initial placement, clamp so the menu stays within the viewport.
      if (buttonEl) {
        const rect = buttonEl.getBoundingClientRect();
        const isVertical = currentPosition === 'left' || currentPosition === 'right';
        // Estimate menu size (actual size checked after mount via requestAnimationFrame)
        const menuW = 190;
        const menuH = 340;
        let top: number;
        let left: number;

        if (isVertical) {
          top = rect.top;
          left = rect.right + 4;
        } else if (currentPosition === 'bottom') {
          // Open upward when tabs are at the bottom
          top = rect.top - menuH - 2;
          left = rect.right - menuW;
        } else {
          top = rect.bottom + 2;
          left = rect.right - menuW;
        }

        // Clamp within viewport
        top = Math.max(4, Math.min(top, window.innerHeight - menuH - 4));
        left = Math.max(4, Math.min(left, window.innerWidth - menuW - 4));

        dropdownStyle = `position:fixed; top:${top}px; left:${left}px;`;
      }
    }
    open = !open;
  }

  function closeMenu() {
    open = false;
  }

  function togglePanel(id: string) {
    if (visibleIds.has(id)) {
      removePanel(id);
      visibleIds.delete(id);
      visibleIds = new Set(visibleIds); // trigger reactivity
    } else {
      addPanel(id, group);
      visibleIds.add(id);
      visibleIds = new Set(visibleIds);
    }
  }

  function setPosition(pos: DockviewHeaderPosition) {
    group.api.setHeaderPosition(pos);
    currentPosition = pos;
    // Explicitly save — setHeaderPosition may not fire onDidLayoutChange in v5
    saveLayout();
    // Close and reopen so the dropdown repositions correctly
    open = false;
  }

  let devMode = $derived(getSetting<boolean>('dev_mode') ?? false);
  let visibleDefs = $derived(PANEL_DEFS.filter(d => !d.devOnly || devMode));

  const TAB_POSITIONS: { label: string; value: DockviewHeaderPosition }[] = [
    { label: 'Top', value: 'top' },
    { label: 'Bottom', value: 'bottom' },
    { label: 'Left', value: 'left' },
    { label: 'Right', value: 'right' },
  ];
</script>

<div class="group-header-menu">
  <button
    class="menu-trigger"
    bind:this={buttonEl}
    onclick={toggleMenu}
    title="Group options"
    aria-label="Group options"
    aria-haspopup="true"
    aria-expanded={open}
  >&#x22EE;</button>

  {#if open}
    <!-- backdrop for click-outside -->
    <div class="menu-backdrop" role="presentation" onclick={closeMenu}></div>
    <div class="menu-dropdown" style={dropdownStyle}>
      <div class="menu-section">
        <div class="menu-section-title">Panels</div>
        {#each visibleDefs as def}
          <label class="menu-item">
            <input
              type="checkbox"
              checked={visibleIds.has(def.id)}
              onchange={() => togglePanel(def.id)}
            />
            <span>{def.title}</span>
          </label>
        {/each}
      </div>
      <div class="menu-divider"></div>
      <div class="menu-section">
        <div class="menu-section-title">Tab Position</div>
        {#each TAB_POSITIONS as pos}
          <label class="menu-item">
            <input
              type="radio"
              name="tab-pos-{group.id}"
              checked={currentPosition === pos.value}
              onchange={() => setPosition(pos.value)}
            />
            <span>{pos.label}</span>
          </label>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .group-header-menu {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .menu-trigger {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 14px;
    padding: 2px 6px;
    line-height: 1;
    border-radius: 3px;
    opacity: 0.6;
    transition: opacity 0.15s, background 0.15s;
  }

  .menu-trigger:hover {
    opacity: 1;
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 999;
  }

  .menu-dropdown {
    z-index: 1000;
    min-width: 180px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 4px 0;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    font-size: 12px;
  }

  .menu-section-title {
    padding: 4px 12px 2px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    user-select: none;
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    cursor: pointer;
    color: var(--text-secondary);
    user-select: none;
  }

  .menu-item:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .menu-item input[type='checkbox'],
  .menu-item input[type='radio'] {
    accent-color: var(--accent-color);
    margin: 0;
  }

  .menu-divider {
    height: 1px;
    background: var(--border-color);
    margin: 4px 0;
  }
</style>
