<script lang="ts">
  import { api } from './api';
  import { getSetting } from './settingsStore.svelte';
  import type { BenchmarkResults, SuiteResult } from './types';

  const ALL_SUITES = [
    { id: 'write_throughput', label: 'Write Throughput' },
    { id: 'read_throughput', label: 'Read Throughput' },
    { id: 'edit_throughput', label: 'Edit Throughput' },
    { id: 'search_performance', label: 'Search' },
    { id: 'namespace_traversal', label: 'Namespace' },
    { id: 'links_backlinks', label: 'Links & Backlinks' },
    { id: 'hierarchy_operations', label: 'Hierarchy' },
    { id: 'edge_operations', label: 'Edges' },
  ];

  let devMode = $derived(getSetting<boolean>('dev_mode') ?? false);
  let running = $state(false);
  let results: BenchmarkResults | null = $state(null);
  let error: string | null = $state(null);
  let selected: Set<string> = $state(new Set());

  function toggleSuite(id: string) {
    const next = new Set(selected);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    selected = next;
  }

  function selectAll() {
    selected = new Set(ALL_SUITES.map(s => s.id));
  }

  function selectNone() {
    selected = new Set();
  }

  async function runBenchmarks(suites?: string[]) {
    running = true;
    error = null;
    results = null;
    try {
      results = await api.debug.runBenchmarks(suites);
    } catch (e: any) {
      error = e.message || 'Benchmark failed';
    } finally {
      running = false;
    }
  }

  function handleRunAll() {
    runBenchmarks();
  }

  function handleRunSelected() {
    const list = Array.from(selected);
    if (list.length > 0) {
      runBenchmarks(list);
    }
  }

  function fmtMs(ms: number): string {
    if (ms < 1) return `${(ms * 1000).toFixed(0)}us`;
    if (ms < 1000) return `${ms.toFixed(1)}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
  }

  function fmtOps(ops: number): string {
    if (ops >= 1000) return `${(ops / 1000).toFixed(1)}k`;
    return ops.toFixed(0);
  }
</script>

<div class="bench-panel">
  <div class="bp-header">
    <span class="bp-title">Benchmarks</span>
    {#if results}
      <span class="bp-total">{fmtMs(results.total_duration_ms)} total</span>
    {/if}
  </div>

  {#if !devMode}
    <div class="bp-empty">Enable Dev Mode in settings to run benchmarks</div>
  {:else}
    <div class="bp-controls">
      <div class="bp-suites">
        {#each ALL_SUITES as suite (suite.id)}
          <label class="bp-suite-check">
            <input
              type="checkbox"
              checked={selected.has(suite.id)}
              onchange={() => toggleSuite(suite.id)}
              disabled={running}
            />
            {suite.label}
          </label>
        {/each}
        <div class="bp-select-links">
          <button class="bp-link" onclick={selectAll} aria-label="Select all suites">All</button>
          <button class="bp-link" onclick={selectNone} aria-label="Clear selection">None</button>
        </div>
      </div>

      <div class="bp-buttons">
        <button class="bp-btn" onclick={handleRunAll} disabled={running}>
          {running ? 'Running...' : 'Run All'}
        </button>
        <button
          class="bp-btn bp-btn-secondary"
          onclick={handleRunSelected}
          disabled={running || selected.size === 0}
        >
          Run Selected ({selected.size})
        </button>
      </div>
    </div>

    {#if running}
      <div class="bp-spinner">Running benchmarks...</div>
    {/if}

    {#if error}
      <div class="bp-error">{error}</div>
    {/if}

    {#if results}
      <div class="bp-results">
        {#each results.suites as suite (suite.name)}
          <div class="bp-suite">
            <div class="bp-suite-header">
              <span class="bp-suite-name">{suite.name}</span>
              <span class="bp-suite-time">{fmtMs(suite.duration_ms)}</span>
            </div>
            <div class="bp-suite-desc">{suite.description}</div>
            <table class="bp-table">
              <thead>
                <tr>
                  <th>Benchmark</th>
                  <th>Ops</th>
                  <th>Duration</th>
                  <th>Ops/sec</th>
                </tr>
              </thead>
              <tbody>
                {#each suite.benchmarks as bench (bench.name)}
                  <tr>
                    <td class="bp-bench-name">{bench.name}</td>
                    <td class="bp-num">{fmtOps(bench.ops)}</td>
                    <td class="bp-num">{fmtMs(bench.duration_ms)}</td>
                    <td class="bp-num">{fmtOps(bench.ops_per_sec)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .bench-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
    overflow-y: auto;
  }

  .bp-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
  }

  .bp-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .bp-total {
    font-size: 10px;
    background: var(--bg-tertiary);
    color: var(--text-muted);
    padding: 0 6px;
    border-radius: 8px;
    margin-left: auto;
  }

  .bp-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 80px;
    color: var(--text-muted);
    font-size: 12px;
  }

  .bp-controls {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-color);
  }

  .bp-suites {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin-bottom: 8px;
  }

  .bp-suite-check {
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .bp-suite-check input {
    margin: 0;
  }

  .bp-select-links {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  .bp-link {
    font-size: 10px;
    color: var(--text-muted);
    cursor: pointer;
    text-decoration: underline;
    background: none;
    border: none;
    padding: 0;
    font-family: inherit;
  }

  .bp-link:hover {
    color: var(--accent-color);
  }

  .bp-buttons {
    display: flex;
    gap: 8px;
  }

  .bp-btn {
    padding: 4px 12px;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    background: var(--accent-color);
    color: var(--bg-primary);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }

  .bp-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .bp-btn-secondary {
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }

  .bp-spinner {
    padding: 16px;
    text-align: center;
    color: var(--text-muted);
    font-size: 12px;
  }

  .bp-error {
    padding: 8px 12px;
    background: rgba(247, 118, 142, 0.1);
    color: #f7768e;
    font-size: 11px;
    border-bottom: 1px solid var(--border-color);
  }

  .bp-results {
    flex: 1;
    padding: 4px 0;
  }

  .bp-suite {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-color);
  }

  .bp-suite-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 2px;
  }

  .bp-suite-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--accent-color);
  }

  .bp-suite-time {
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .bp-suite-desc {
    font-size: 10px;
    color: var(--text-muted);
    margin-bottom: 6px;
  }

  .bp-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11px;
    font-family: var(--font-mono);
  }

  .bp-table th {
    text-align: left;
    padding: 2px 6px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-bottom: 1px solid var(--border-color);
  }

  .bp-table td {
    padding: 2px 6px;
    white-space: nowrap;
  }

  .bp-table tr:hover {
    background: var(--bg-hover);
  }

  .bp-bench-name {
    color: var(--text-primary);
  }

  .bp-num {
    text-align: right;
    color: var(--text-secondary);
  }
</style>
