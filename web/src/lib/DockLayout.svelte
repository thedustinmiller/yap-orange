<script lang="ts">
  import { onMount } from 'svelte';
  import { mount, unmount } from 'svelte';
  import {
    DockviewComponent,
    themeDark,
    type DockviewGroupPanel,
    type IContentRenderer,
    type IHeaderActionsRenderer,
    type SerializedDockview,
  } from 'dockview-core';
  import 'dockview-core/dist/styles/dockview.css';
  import {
    setDockviewRef,
    PANEL_DEFS,
    DEV_PANEL_IDS,
    saveLayout as saveLayoutShared,
    showAndFocusPanel,
    getActivePanelId,
    removePanel,
  } from './dockviewActions.svelte';
  import { getSetting } from './settingsStore.svelte';
  import {
    createOutliner,
    removeOutliner,
    setActiveOutliner,
    outlinerState,
  } from './outlinerStore.svelte';
  import { appState } from './appState.svelte';
  import { pushRoute } from './router.svelte';
  import { pushPanel, getLastOutlinerId, removeFromMru } from './panelHistory.svelte';
  import QuickSwitcher from './QuickSwitcher.svelte';

  /** Check if a panel ID belongs to an outliner. */
  function isOutlinerPanel(id: string): boolean {
    return id === 'outliner' || id.startsWith('outliner-') || id.startsWith('ol-');
  }

  import Sidebar from './Sidebar.svelte';
  import Outliner from './Outliner.svelte';
  import Bookmarks from './Bookmarks.svelte';
  import GraphPreview from './GraphPreview.svelte';
  import Backlinks from './Backlinks.svelte';
  import Properties from './Properties.svelte';
  import DebugLog from './DebugLog.svelte';
  import BenchmarkPanel from './BenchmarkPanel.svelte';
  import ImportExport from './ImportExport.svelte';
  import GroupHeaderMenu from './GroupHeaderMenu.svelte';

  interface Props {
    savedLayout?: SerializedDockview | null;
  }

  let { savedLayout = null }: Props = $props();

  let containerEl: HTMLDivElement | undefined = $state();
  let dockview: DockviewComponent | undefined;
  let showQuickSwitcher = $state(false);

  /** Alt+1..5 shortcut targets (in order). */
  const ALT_PANEL_MAP: Record<string, string> = {
    '1': 'sidebar',
    '2': 'bookmarks',
    '3': 'backlinks',
    '4': 'properties',
    '5': 'graph',
  };

  /**
   * Toggle logic for Alt+N panel shortcuts:
   * - Panel hidden/removed -> show + focus
   * - Panel visible but not active -> focus
   * - Panel already focused -> bounce back to last outliner
   */
  function togglePanel(panelId: string): void {
    const activeId = getActivePanelId();
    if (activeId === panelId) {
      // Already focused — bounce to last outliner
      const outliner = getLastOutlinerId();
      if (outliner && dockview) {
        const panel = dockview.panels.find(p => p.id === outliner);
        if (panel) panel.api.setActive();
      }
    } else {
      showAndFocusPanel(panelId);
    }
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    // Alt+1..5: panel shortcuts
    if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
      const panelId = ALT_PANEL_MAP[e.key];
      if (panelId) {
        e.preventDefault();
        togglePanel(panelId);
        return;
      }
    }

    // Ctrl+K / Cmd+K: quick switcher
    if ((e.ctrlKey || e.metaKey) && e.key === 'k' && !e.altKey && !e.shiftKey) {
      e.preventDefault();
      showQuickSwitcher = !showQuickSwitcher;
      return;
    }
  }

  /**
   * Creates a dockview IContentRenderer that mounts a Svelte 5 component
   * into the panel's DOM element.
   */
  function createSvelteRenderer(
    Component: any,
    props: Record<string, any> = {}
  ): IContentRenderer {
    const element = document.createElement('div');
    element.style.height = '100%';
    element.style.width = '100%';
    element.style.overflow = 'hidden';

    let cleanup: (() => void) | undefined;

    return {
      element,
      init(_params) {
        const instance = mount(Component, {
          target: element,
          props,
        });
        cleanup = () => {
          try {
            unmount(instance);
          } catch {
            // Intentional: component may already be unmounted
          }
        };
      },
      dispose() {
        cleanup?.();
      },
    };
  }

  const COMPONENT_MAP: Record<string, any> = {
    sidebar: Sidebar,
    bookmarks: Bookmarks,
    graph: GraphPreview,
    backlinks: Backlinks,
    properties: Properties,
    debuglog: DebugLog,
    benchmarks: BenchmarkPanel,
    importexport: ImportExport,
  };

  function createComponent(options: { id: string; name: string }): IContentRenderer {
    // Dynamic outliner panels — name is 'outliner', id varies
    if (options.name === 'outliner') {
      const instance = createOutliner(options.id);
      return createSvelteRenderer(Outliner, { outlinerId: instance.id });
    }

    const Component = COMPONENT_MAP[options.name];
    if (Component) return createSvelteRenderer(Component);

    const el = document.createElement('div');
    el.textContent = `Unknown panel: ${options.name}`;
    return { element: el, init() {} };
  }

  /** Build the default layout by adding panels imperatively. */
  function buildDefaultLayout(dv: DockviewComponent) {
    const sidebarPanel = dv.addPanel({
      id: 'sidebar',
      component: 'sidebar',
      title: 'Navigator',
    });

    const outlinerPanel = dv.addPanel({
      id: 'outliner',
      component: 'outliner',
      title: 'Outliner',
      position: { referencePanel: sidebarPanel, direction: 'right' },
    });

    dv.addPanel({
      id: 'bookmarks',
      component: 'bookmarks',
      title: 'Bookmarks',
      position: { referencePanel: sidebarPanel, direction: 'below' },
    });

    const graphPanel = dv.addPanel({
      id: 'graph',
      component: 'graph',
      title: 'Graph',
      position: { referencePanel: outlinerPanel, direction: 'right' },
    });

    const backlinksPanel = dv.addPanel({
      id: 'backlinks',
      component: 'backlinks',
      title: 'Links',
      position: { referencePanel: graphPanel, direction: 'below' },
    });

    dv.addPanel({
      id: 'properties',
      component: 'properties',
      title: 'Properties',
      position: { referencePanel: backlinksPanel, direction: 'within' },
    });

    // Import/Export lives as a background tab in the outliner group
    dv.addPanel({
      id: 'importexport',
      component: 'importexport',
      title: 'Import/Export',
      position: { referencePanel: outlinerPanel, direction: 'within' },
    });
    // Re-activate the outliner so it stays in front
    outlinerPanel.api.setActive();

    if (getSetting<boolean>('dev_mode')) {
      const debuglogPanel = dv.addPanel({
        id: 'debuglog',
        component: 'debuglog',
        title: 'Debug Log',
        position: { referencePanel: outlinerPanel, direction: 'below' },
      });

      dv.addPanel({
        id: 'benchmarks',
        component: 'benchmarks',
        title: 'Benchmarks',
        position: { referencePanel: debuglogPanel, direction: 'within' },
      });
    }

    // Set initial proportions
    requestAnimationFrame(() => {
      if (!containerEl || !dv) return;
      dv.layout(containerEl.offsetWidth, containerEl.offsetHeight);

      try {
        const sidebarGroup = dv.groups.find(g =>
          g.panels.some(p => p.id === 'sidebar')
        );
        const outlinerGroup = dv.groups.find(g =>
          g.panels.some(p => isOutlinerPanel(p.id))
        );
        sidebarGroup?.api.setSize({ width: 220 });
        outlinerGroup?.api.setSize({ width: Math.max(400, containerEl.offsetWidth - 540) });
      } catch {
        // Intentional: sizing API might not be available during transitions
      }
    });
  }

  /** Restore layout from serialized state, falling back to default on error. */
  function restoreLayout(dv: DockviewComponent, data: SerializedDockview) {
    try {
      dv.fromJSON(data);

      // Ensure all expected non-outliner panels exist (skip dev panels when dev_mode is off)
      const devMode = getSetting<boolean>('dev_mode') ?? false;
      const existingIds = new Set(dv.panels.map(p => p.id));
      for (const def of PANEL_DEFS) {
        if (def.devOnly && !devMode) {
          // Remove dev panels that were saved in layout but dev_mode is now off
          if (existingIds.has(def.id)) {
            const panel = dv.panels.find(p => p.id === def.id);
            if (panel) panel.api.close();
          }
          continue;
        }
        if (!existingIds.has(def.id)) {
          dv.addPanel({
            id: def.id,
            component: def.component,
            title: def.title,
          });
        }
      }

      // Ensure at least one outliner exists
      const hasOutliner = dv.panels.some(p => isOutlinerPanel(p.id));
      if (!hasOutliner) {
        dv.addPanel({
          id: 'outliner',
          component: 'outliner',
          title: 'Outliner',
        });
      }

      // Re-activate outliner panels in their groups so newly-added panels
      // (e.g. importexport) don't steal the active tab from the outliner
      for (const panel of dv.panels) {
        if (isOutlinerPanel(panel.id)) {
          panel.api.setActive();
        }
      }
    } catch (err) {
      console.warn('Failed to restore layout, using default:', err);
      dv.clear();
      buildDefaultLayout(dv);
    }
  }

  /**
   * Factory for the "⋮" menu button in each group's tab bar.
   * Mounts a Svelte 5 GroupHeaderMenu into the header actions slot.
   */
  function createRightHeaderActionComponent(
    group: DockviewGroupPanel,
  ): IHeaderActionsRenderer {
    const element = document.createElement('div');
    element.style.height = '100%';
    element.style.display = 'flex';
    element.style.alignItems = 'center';

    let cleanup: (() => void) | undefined;

    return {
      element,
      init(_params) {
        const instance = mount(GroupHeaderMenu, {
          target: element,
          props: { group },
        });
        cleanup = () => {
          try {
            unmount(instance);
          } catch {
            // Component may already be unmounted
          }
        };
      },
      dispose() {
        cleanup?.();
      },
    };
  }

  /** Save layout state to settings (debounced via shared helper). */
  function saveLayout(_dv: DockviewComponent) {
    saveLayoutShared();
  }

  onMount(() => {
    if (!containerEl) return;

    dockview = new DockviewComponent(containerEl, {
      createComponent,
      createRightHeaderActionComponent,
      disableFloatingGroups: true,
      theme: themeDark,
    });

    // Expose dockview ref so other modules can add panels
    setDockviewRef(dockview);

    if (savedLayout) {
      restoreLayout(dockview, savedLayout);
    } else {
      buildDefaultLayout(dockview);
    }

    // Track active panel for outliner focus switching + MRU history
    const activePanelDisposable = dockview.onDidActivePanelChange((e) => {
      if (!e) return;
      pushPanel(e.id);
      if (isOutlinerPanel(e.id)) {
        setActiveOutliner(e.id, appState);
        // No pushRoute here — which tab is active is dockview layout state,
        // not encoded in the URL. The URL only changes on actual navigation.
        const name = appState.activeNamespaceName;
        const title = name ? `Outliner - ${name}` : 'Outliner';
        e.api.setTitle(title);
      }
    });

    // Clean up outliner instances and MRU when panels are removed
    const removePanelDisposable = dockview.onDidRemovePanel((e) => {
      removeFromMru(e.id);
      if (isOutlinerPanel(e.id)) {
        removeOutliner(e.id);
      }
    });

    // Listen for layout changes and persist (debounced via setSetting)
    const layoutDisposable = dockview.onDidLayoutChange(() => {
      if (dockview) saveLayout(dockview);
    });

    const resizeObserver = new ResizeObserver(() => {
      if (containerEl && dockview) {
        dockview.layout(containerEl.offsetWidth, containerEl.offsetHeight);
      }
    });
    resizeObserver.observe(containerEl);

    // Global keyboard shortcuts (Alt+N, Ctrl+K)
    window.addEventListener('keydown', handleGlobalKeydown);

    return () => {
      window.removeEventListener('keydown', handleGlobalKeydown);
      activePanelDisposable?.dispose();
      removePanelDisposable?.dispose();
      layoutDisposable?.dispose();
      resizeObserver.disconnect();
      setDockviewRef(null);
      dockview?.dispose();
    };
  });

  // Reactively update the active outliner's tab title when navigation changes
  $effect(() => {
    const activeId = outlinerState.activeOutlinerId;
    const name = appState.activeNamespaceName;
    if (!dockview || !activeId) return;
    const title = name ? `Outliner - ${name}` : 'Outliner';
    const panel = dockview.panels.find(p => p.id === activeId);
    if (panel) {
      panel.api.setTitle(title);
    }
  });

  // Push multi-route URL whenever the active outliner's path changes
  $effect(() => {
    const path = appState.activeNamespaceFullPath;
    const activeId = outlinerState.activeOutlinerId;
    if (!dockview || !activeId) return;
    // pushRoute handles single vs multi serialization internally
    pushRoute(path ?? '/');
  });

  // Close dev-only panels when dev_mode is toggled off
  $effect(() => {
    const devMode = getSetting<boolean>('dev_mode') ?? false;
    if (!dockview || devMode) return;
    for (const id of DEV_PANEL_IDS) {
      removePanel(id);
    }
  });
</script>

{#if showQuickSwitcher}
  <QuickSwitcher onClose={() => showQuickSwitcher = false} />
{/if}

<div class="dock-layout" bind:this={containerEl}></div>

<style>
  .dock-layout {
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  /* Compound selector beats dockview v5's runtime-injected .dockview-theme-dark */
  .dock-layout :global(.dockview-theme-dark) {
    --dv-paneview-header-border-color: var(--border-color);
    --dv-tabs-and-actions-container-background-color: var(--bg-secondary);
    --dv-activegroup-visiblepanel-tab-background-color: var(--bg-primary);
    --dv-activegroup-hiddenpanel-tab-background-color: var(--bg-secondary);
    --dv-inactivegroup-visiblepanel-tab-background-color: var(--bg-tertiary);
    --dv-inactivegroup-hiddenpanel-tab-background-color: var(--bg-secondary);
    --dv-inactivegroup-visiblepanel-tab-color: var(--text-secondary);
    --dv-inactivegroup-hiddenpanel-tab-color: var(--text-muted);
    --dv-tab-divider-color: var(--border-color);
    --dv-activegroup-visiblepanel-tab-color: var(--text-primary);
    --dv-activegroup-hiddenpanel-tab-color: var(--text-muted);
    --dv-separator-border: var(--border-color);
    --dv-group-view-background-color: var(--bg-primary);
    --dv-paneview-active-outline-color: var(--border-color);
  }
</style>
