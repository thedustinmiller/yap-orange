<script lang="ts">
  import { api } from './api';
  import { getSetting } from './settingsStore.svelte';

  interface LogEntry {
    id: number;
    timestamp: string;
    level: string;
    target: string;
    message: string;
  }

  let entries: LogEntry[] = $state([]);
  let lastId = 0;
  let polling = false;
  let autoScroll = $state(true);
  let contentEl: HTMLDivElement | undefined = $state();
  let devMode = $derived(getSetting<boolean>('dev_mode') ?? false);

  async function poll() {
    if (polling) return; // prevent concurrent polls
    polling = true;
    try {
      const newEntries: LogEntry[] = await api.debug.logs(lastId);
      if (newEntries.length > 0) {
        lastId = newEntries[newEntries.length - 1].id;
        // Deduplicate by ID (guard against concurrent poll edge cases)
        const seen = new Set(entries.map(e => e.id));
        const unique = newEntries.filter(e => !seen.has(e.id));
        entries = [...entries, ...unique].slice(-500);
        if (autoScroll) {
          requestAnimationFrame(() => {
            contentEl?.scrollTo(0, contentEl.scrollHeight);
          });
        }
      }
    } catch {
      // Server may not be ready
    } finally {
      polling = false;
    }
  }

  // Svelte 5 $effect: return cleanup function to clear interval on re-run or destroy
  $effect(() => {
    if (devMode) {
      poll();
      const timer = setInterval(poll, 1000);
      return () => clearInterval(timer);
    }
  });

  function handleScroll() {
    if (!contentEl) return;
    const atBottom = contentEl.scrollHeight - contentEl.scrollTop - contentEl.clientHeight < 40;
    autoScroll = atBottom;
  }

  function handleClear() {
    entries = [];
  }

  function levelClass(level: string): string {
    switch (level) {
      case 'ERROR': return 'log-error';
      case 'WARN': return 'log-warn';
      case 'INFO': return 'log-info';
      case 'DEBUG': return 'log-debug';
      case 'TRACE': return 'log-trace';
      default: return '';
    }
  }
</script>

<div class="debug-log-panel">
  <div class="dl-header">
    <span class="dl-title">Debug Log</span>
    {#if entries.length > 0}
      <span class="dl-count">{entries.length}</span>
    {/if}
    <div class="dl-actions">
      <button class="dl-action" onclick={handleClear} aria-label="Clear log">Clear</button>
    </div>
  </div>

  <div
    class="dl-content"
    bind:this={contentEl}
    onscroll={handleScroll}
  >
    {#if !devMode}
      <div class="dl-empty">Enable Dev Mode in settings to see server logs</div>
    {:else if entries.length === 0}
      <div class="dl-empty">Waiting for log entries...</div>
    {:else}
      {#each entries as entry (entry.id)}
        <div class="dl-entry {levelClass(entry.level)}">
          <span class="dl-ts">{entry.timestamp.slice(11, 23)}</span>
          <span class="dl-level">{entry.level.padEnd(5)}</span>
          <span class="dl-target">{entry.target}</span>
          <span class="dl-msg">{entry.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .debug-log-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
  }

  .dl-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
  }

  .dl-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .dl-count {
    font-size: 10px;
    background: var(--bg-tertiary);
    color: var(--text-muted);
    padding: 0 6px;
    border-radius: 8px;
  }

  .dl-actions {
    margin-left: auto;
  }

  .dl-action {
    background: none;
    border: none;
    padding: 0;
    font-family: inherit;
    font-size: 11px;
    color: var(--text-muted);
    cursor: pointer;
    transition: color 0.1s;
  }

  .dl-action:hover {
    color: var(--accent-color);
  }

  .dl-content {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.5;
  }

  .dl-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 80px;
    color: var(--text-muted);
    font-size: 12px;
    font-family: inherit;
  }

  .dl-entry {
    display: flex;
    gap: 6px;
    padding: 1px 8px;
    white-space: nowrap;
  }

  .dl-entry:hover {
    background: var(--bg-hover);
  }

  .dl-ts {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .dl-level {
    flex-shrink: 0;
    width: 5ch;
    font-weight: 600;
  }

  .dl-target {
    color: var(--text-muted);
    flex-shrink: 0;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dl-msg {
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Level-specific colors */
  .log-error .dl-level { color: #f7768e; }
  .log-warn .dl-level { color: #e0af68; }
  .log-info .dl-level { color: #7aa2f7; }
  .log-debug .dl-level { color: #9ece6a; }
  .log-trace .dl-level { color: var(--text-muted); }

  .log-error .dl-msg { color: #f7768e; }
  .log-warn .dl-msg { color: #e0af68; }
</style>
