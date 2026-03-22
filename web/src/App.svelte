<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import DockLayout from './lib/DockLayout.svelte';
  import Toast from './lib/Toast.svelte';
  import { api } from './lib/api';
  import { appState, navigateTo, navigateHome, registerRoutePusher } from './lib/appState.svelte';
  import { loadSettings, getSetting, setSetting, isLoaded, flushSettingsNow } from './lib/settingsStore.svelte';
  import { loadBookmarks } from './lib/bookmarkStore.svelte';
  import { initRouter, handleInitialHash, pushRoute } from './lib/router.svelte';
  import type { SerializedDockview } from 'dockview-core';

  // Register pushRoute with appState before any navigation can happen.
  // This runs synchronously during component init, before onMount.
  registerRoutePusher(pushRoute);

  // Guard: only write settings after the restore phase is complete.
  // Prevents the $effects below from overwriting restored values before
  // they've been read from the server.
  let settingsReady = $state(false);
  let savedLayout = $state<SerializedDockview | null>(null);

  // Cleanup handlers — registered in onMount, torn down in onDestroy.
  // Kept outside onMount because async onMount can't return a cleanup function.
  let cleanupFns: (() => void)[] = [];
  onDestroy(() => cleanupFns.forEach(fn => fn()));

  onMount(async () => {
    // 1. Health check
    try {
      const health = await api.health();
      appState.serverConnected = health.status === 'ok';
    } catch {
      appState.serverConnected = false;
    }

    // 2. Load settings from server (no-op if already loaded)
    await loadSettings();

    // 2b. Load bookmarks from settings
    loadBookmarks();

    // 2c. Restore saved layout (must happen before DockLayout mounts)
    savedLayout = getSetting<SerializedDockview>('layout_state') ?? null;

    // 3. Restore expand state for sidebar
    const sidebarExpanded = getSetting<string[]>('sidebar_expanded') ?? [];
    for (const id of sidebarExpanded) appState.sidebarExpanded.add(id);

    // 4. Restore expand state for outliner
    const outlinerExpanded = getSetting<string[]>('outliner_expanded') ?? [];
    for (const id of outlinerExpanded) appState.expandedBlocks.add(id);

    // 5. Allow $effects to start persisting now that state is restored
    settingsReady = true;

    // 6. Init router (sets up hashchange listener)
    initRouter();

    // 7. Hash takes priority — if present, navigate there
    const hasHash = await handleInitialHash();
    if (!hasHash) {
      // 8. Fall back to last-location setting
      const lastLocation = getSetting<{ block_id: string }>('last_location');
      if (lastLocation?.block_id) {
        try {
          await navigateTo(lastLocation.block_id);
        } catch {
          navigateHome();
        }
      }
      // If neither hash nor last-location: stay at home (default state)
    }

    // 9. Navigation complete — now load the sidebar namespace tree
    appState.navigationReady = true;

    // 10. Warn on unsaved changes before page unload
    function handleBeforeUnload(e: BeforeUnloadEvent) {
      if (appState.hasUnsavedChanges) {
        e.preventDefault();
      }
    }
    window.addEventListener('beforeunload', handleBeforeUnload);

    // 11. Flush pending settings on page unload so debounced changes aren't lost
    function handleUnload() {
      flushSettingsNow();
    }
    window.addEventListener('unload', handleUnload);

    cleanupFns.push(() => {
      window.removeEventListener('beforeunload', handleBeforeUnload);
      window.removeEventListener('unload', handleUnload);
    });
  });

  // Persist sidebar expand state on change (debounced 2s)
  $effect(() => {
    if (!settingsReady) return;
    const expanded = [...appState.sidebarExpanded];
    setSetting('sidebar_expanded', expanded, 2000);
  });

  // Persist outliner expand state on change (debounced 2s)
  $effect(() => {
    if (!settingsReady) return;
    const expanded = [...appState.expandedBlocks];
    setSetting('outliner_expanded', expanded, 2000);
  });

  // Persist last-location on navigation (debounced 1s)
  $effect(() => {
    if (!settingsReady) return;
    const blockId = appState.activeNamespaceBlockId;
    const namespace = appState.activeNamespaceFullPath;
    if (blockId) {
      setSetting('last_location', { block_id: blockId, namespace }, 1000);
    }
  });
</script>

<svelte:head><title>yap-orange</title></svelte:head>

<main class="app-root theme-dark">
  {#if settingsReady}
    <DockLayout {savedLayout} />
  {/if}
</main>
<div aria-live="polite">
  <Toast />
</div>

<style>
  .app-root {
    width: 100%;
    height: 100%;
  }
</style>
