<script lang="ts">
  import { SvelteFlowProvider } from '@xyflow/svelte';
  import GraphFlow from './GraphFlow.svelte';
  import { getSetting, setSetting } from './settingsStore.svelte';

  let mode: 'links' | 'outliner' = $state(
    (getSetting<string>('graph_mode') as 'links' | 'outliner') || 'links'
  );

  function setMode(m: 'links' | 'outliner') {
    mode = m;
    setSetting('graph_mode', m);
  }
</script>

<div class="graph-preview">
  <div class="graph-header">
    <span class="graph-title">Graph</span>
    <div class="mode-toggle" role="radiogroup" aria-label="Graph mode">
      <button
        class="mode-btn"
        class:active={mode === 'links'}
        role="radio"
        aria-checked={mode === 'links'}
        onclick={() => setMode('links')}
      >Links</button>
      <button
        class="mode-btn"
        class:active={mode === 'outliner'}
        role="radio"
        aria-checked={mode === 'outliner'}
        onclick={() => setMode('outliner')}
      >Outliner</button>
    </div>
  </div>
  <div class="graph-container">
    <SvelteFlowProvider>
      <GraphFlow {mode} />
    </SvelteFlowProvider>
  </div>

  <div class="graph-legend">
    <div class="legend-item">
      <span class="legend-line animated"></span>
      <span>content link</span>
    </div>
    <div class="legend-item">
      <span class="legend-line dashed"></span>
      <span>semantic edge</span>
    </div>
    {#if mode === 'links'}
      <div class="legend-item">
        <span class="legend-line hard-link"></span>
        <span>hard link</span>
      </div>
    {:else}
      <div class="legend-item">
        <span class="legend-line parent-child"></span>
        <span>parent-child</span>
      </div>
    {/if}
  </div>
</div>

<style>
  .graph-preview {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
  }

  .graph-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
  }

  .graph-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .mode-toggle {
    display: flex;
    gap: 0;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    overflow: hidden;
  }

  .mode-btn {
    padding: 2px 10px;
    font-size: 10px;
    font-weight: 500;
    background: var(--bg-tertiary);
    color: var(--text-muted);
    border: none;
    cursor: pointer;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    transition: all 0.1s;
  }

  .mode-btn:not(:last-child) {
    border-right: 1px solid var(--border-color);
  }

  .mode-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .mode-btn.active {
    background: var(--accent-color);
    color: white;
  }

  .graph-container {
    flex: 1;
    min-height: 200px;
  }

  .graph-legend {
    display: flex;
    gap: 16px;
    padding: 6px 12px;
    border-top: 1px solid var(--border-color);
    font-size: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .legend-line {
    display: inline-block;
    width: 20px;
    height: 2px;
  }

  .legend-line.animated {
    background: var(--link-color);
  }

  .legend-line.dashed {
    background: repeating-linear-gradient(
      90deg,
      var(--edge-color) 0px,
      var(--edge-color) 4px,
      transparent 4px,
      transparent 8px
    );
  }

  .legend-line.hard-link {
    background: repeating-linear-gradient(
      90deg,
      var(--accent-color) 0px,
      var(--accent-color) 3px,
      transparent 3px,
      transparent 6px
    );
  }

  .legend-line.parent-child {
    background: var(--border-color);
  }
</style>
