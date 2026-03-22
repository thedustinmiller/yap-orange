<script lang="ts">
  import { appState, navigateToAtom } from './appState.svelte';
  import { blockTree } from './blockTree.svelte';
  import { pushRoute } from './router.svelte';
  import { api } from './api';
  import { segmentContent } from './content';
  import type { Backlink, AtomGraph, EdgeResponse, HardLink } from './types';

  let outlinks: Backlink[] = $state([]);
  let backlinks: Backlink[] = $state([]);
  let edgesOut: EdgeResponse[] = $state([]);
  let edgesIn: EdgeResponse[] = $state([]);
  let hardLinks: HardLink[] = $state([]);
  let loading = $state(false);

  function getActiveLineageId(): string | null {
    if (appState.selectedBlockIds.length > 0) {
      const node = blockTree.getNode(appState.selectedBlockIds[0]);
      if (node) return node.lineage_id;
    }
    return appState.activeNamespaceLineageId;
  }

  let totalCount = $derived(outlinks.length + backlinks.length + edgesOut.length + edgesIn.length + hardLinks.length);

  let lastLineageId: string | null = null;

  $effect(() => {
    const lineageId = getActiveLineageId();
    if (lineageId === lastLineageId) return;
    lastLineageId = lineageId;

    if (!lineageId) {
      outlinks = [];
      backlinks = [];
      edgesOut = [];
      edgesIn = [];
      hardLinks = [];
      return;
    }

    loading = true;
    api.atoms.graph(lineageId).then((result: AtomGraph) => {
      outlinks = result.outlinks ?? [];
      backlinks = result.backlinks ?? [];
      edgesOut = result.edges?.outgoing ?? [];
      edgesIn = result.edges?.incoming ?? [];
      hardLinks = result.hard_links ?? [];
      loading = false;
    }).catch(() => {
      outlinks = [];
      backlinks = [];
      edgesOut = [];
      edgesIn = [];
      hardLinks = [];
      loading = false;
    });
  });

  function handleClick(lineageId: string) {
    navigateToAtom(lineageId);
  }

  function truncate(content: string, maxLen = 120): string {
    return content.length > maxLen ? content.slice(0, maxLen) + '...' : content;
  }
</script>

<div class="links-panel">
  <div class="lk-header">
    <span class="lk-title">Links</span>
    {#if totalCount > 0}
      <span class="lk-count">{totalCount}</span>
    {/if}
  </div>

  <div class="lk-content">
    {#if !getActiveLineageId()}
      <div class="lk-empty">Select a block to see links</div>
    {:else if loading}
      <div class="lk-empty">Loading...</div>
    {:else if totalCount === 0}
      <div class="lk-empty">No links found</div>
    {:else}

      {#if outlinks.length > 0}
        <div class="lk-section">
          <div class="lk-section-header">
            <span class="lk-section-label">Links to</span>
            <span class="lk-section-count">{outlinks.length}</span>
          </div>
          {#each outlinks as link (link.lineage_id)}
            <button class="lk-item" onclick={() => handleClick(link.lineage_id)}>
              <div class="lk-namespace">{link.namespace ?? 'unknown'}</div>
              <div class="lk-snippet">
                {#each segmentContent(truncate(link.content)) as seg, i}
                  {#if seg.type === 'link'}
                    <span class="lk-link">[[{seg.value}]]</span>
                  {:else}
                    {seg.value}
                  {/if}
                {/each}
              </div>
            </button>
          {/each}
        </div>
      {/if}

      {#if backlinks.length > 0}
        <div class="lk-section">
          <div class="lk-section-header">
            <span class="lk-section-label">Linked from</span>
            <span class="lk-section-count">{backlinks.length}</span>
          </div>
          {#each backlinks as bl (bl.lineage_id)}
            <button class="lk-item" onclick={() => handleClick(bl.lineage_id)}>
              <div class="lk-namespace">{bl.namespace ?? 'unknown'}</div>
              <div class="lk-snippet">
                {#each segmentContent(truncate(bl.content)) as seg, i}
                  {#if seg.type === 'link'}
                    <span class="lk-link">[[{seg.value}]]</span>
                  {:else}
                    {seg.value}
                  {/if}
                {/each}
              </div>
            </button>
          {/each}
        </div>
      {/if}

      {#if edgesOut.length > 0 || edgesIn.length > 0}
        <div class="lk-section">
          <div class="lk-section-header">
            <span class="lk-section-label">Edges</span>
            <span class="lk-section-count">{edgesOut.length + edgesIn.length}</span>
          </div>
          {#each edgesOut as edge (edge.id)}
            <button class="lk-item lk-edge" onclick={() => handleClick(edge.to_lineage_id)}>
              <div class="lk-edge-type">
                <span class="lk-direction">&#x2192;</span>
                <span class="lk-type-label">{edge.edge_type}</span>
              </div>
              <div class="lk-namespace">{edge.to_lineage_id}</div>
            </button>
          {/each}
          {#each edgesIn as edge (edge.id)}
            <button class="lk-item lk-edge" onclick={() => handleClick(edge.from_lineage_id)}>
              <div class="lk-edge-type">
                <span class="lk-direction">&#x2190;</span>
                <span class="lk-type-label">{edge.edge_type}</span>
              </div>
              <div class="lk-namespace">{edge.from_lineage_id}</div>
            </button>
          {/each}
        </div>
      {/if}

      {#if hardLinks.length > 0}
        <div class="lk-section">
          <div class="lk-section-header">
            <span class="lk-section-label">Hard links</span>
            <span class="lk-section-count">{hardLinks.length}</span>
          </div>
          {#each hardLinks as hl (hl.block_id)}
            <button class="lk-item lk-hardlink" onclick={() => pushRoute(hl.namespace)}>
              <div class="lk-hardlink-icon">&#x29C9;</div>
              <div class="lk-hardlink-info">
                <div class="lk-namespace">{hl.namespace}</div>
                <div class="lk-hardlink-name">{hl.name}</div>
              </div>
            </button>
          {/each}
        </div>
      {/if}

    {/if}
  </div>
</div>

<style>
  .links-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
  }

  .lk-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
  }

  .lk-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .lk-count {
    font-size: 10px;
    background: var(--accent-color);
    color: white;
    padding: 0 6px;
    border-radius: 8px;
  }

  .lk-content {
    flex: 1;
    overflow-y: auto;
    padding: 4px;
  }

  .lk-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 80px;
    color: var(--text-muted);
    font-size: 12px;
  }

  .lk-section {
    margin-bottom: 8px;
  }

  .lk-section-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
  }

  .lk-section-label {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .lk-section-count {
    font-size: 9px;
    color: var(--text-muted);
    background: var(--bg-tertiary);
    padding: 0 5px;
    border-radius: 6px;
  }

  .lk-item {
    padding: 6px 8px;
    margin: 1px 0;
    border-radius: 4px;
    cursor: pointer;
    transition: background 0.1s;
    background: none;
    border: none;
    width: 100%;
    text-align: left;
    color: inherit;
    font-family: inherit;
    font-size: inherit;
    display: block;
  }

  .lk-item:hover {
    background: var(--bg-hover);
  }

  .lk-namespace {
    font-size: 10px;
    font-family: var(--font-mono);
    color: var(--accent-color);
    margin-bottom: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .lk-snippet {
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .lk-link {
    color: var(--link-color);
  }

  .lk-edge-type {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 2px;
  }

  .lk-direction {
    font-size: 11px;
    color: var(--text-muted);
  }

  .lk-type-label {
    font-size: 11px;
    color: var(--text-secondary);
    font-style: italic;
  }

  .lk-hardlink {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .lk-hardlink-icon {
    font-size: 14px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .lk-hardlink-info {
    min-width: 0;
  }

  .lk-hardlink-name {
    font-size: 11px;
    color: var(--text-secondary);
  }
</style>
