<script lang="ts">
  import { api } from './api';
  import { appState } from './appState.svelte';
  import { blockTree } from './blockTree.svelte';

  let exportStatus = $state('');
  let importStatus = $state('');
  let selectedFileName = $state('');
  let selectedData: any = $state(null);
  let importMode: 'merge' | 'copy' = $state('merge');
  let matchStrategy = $state('');
  let exporting = $state(false);
  let importing = $state(false);
  let globalLink = $state(false);
  let importAtRoot = $state(false);

  // Export property key filtering
  let propertyKeys: string[] = $state([]);
  let selectedKeys: Set<string> = $state(new Set());
  let loadingKeys = $state(false);

  let activeBlockId = $derived(appState.activeNamespaceBlockId);
  let activePath = $derived(appState.activeNamespaceFullPath ?? '');

  // Load property keys when active block changes
  $effect(() => {
    const blockId = activeBlockId;
    if (blockId) {
      loadPropertyKeys(blockId);
    } else {
      propertyKeys = [];
      selectedKeys = new Set();
    }
  });

  async function loadPropertyKeys(blockId: string) {
    loadingKeys = true;
    try {
      const keys = await api.importExport.propertyKeys(blockId);
      propertyKeys = keys;
      // Default: select all non-underscore keys
      selectedKeys = new Set(keys.filter(k => !k.startsWith('_')));
    } catch {
      console.warn('Failed to load property keys');
      propertyKeys = [];
      selectedKeys = new Set();
    } finally {
      loadingKeys = false;
    }
  }

  function toggleKey(key: string) {
    if (key === 'name') return; // name is always included
    const next = new Set(selectedKeys);
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    selectedKeys = next;
  }

  // ── File I/O helpers ──────────────────────────

  async function saveJsonFile(data: any, suggestedName: string) {
    if ('showSaveFilePicker' in window) {
      const handle = await (window as any).showSaveFilePicker({
        suggestedName,
        types: [{ description: 'JSON', accept: { 'application/json': ['.json'] } }],
      });
      const writable = await handle.createWritable();
      await writable.write(JSON.stringify(data, null, 2));
      await writable.close();
    } else {
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = suggestedName;
      a.click();
      URL.revokeObjectURL(a.href);
    }
  }

  let selectedZipFile: File | null = $state(null);

  async function openImportFile(): Promise<any> {
    if ('showOpenFilePicker' in window) {
      const [handle] = await (window as any).showOpenFilePicker({
        types: [
          { description: 'JSON or ZIP', accept: { 'application/json': ['.json'], 'application/zip': ['.zip'] } },
        ],
      });
      const file = await handle.getFile();
      selectedFileName = file.name;
      if (file.name.endsWith('.zip')) {
        selectedZipFile = file;
        return null; // ZIP file — handled separately
      }
      selectedZipFile = null;
      return JSON.parse(await file.text());
    } else {
      return new Promise((resolve, reject) => {
        const input = document.createElement('input');
        input.type = 'file';
        input.accept = '.json,.zip';
        input.onchange = async () => {
          const file = input.files?.[0];
          if (!file) return reject(new Error('No file selected'));
          selectedFileName = file.name;
          if (file.name.endsWith('.zip')) {
            selectedZipFile = file;
            resolve(null);
          } else {
            selectedZipFile = null;
            resolve(JSON.parse(await file.text()));
          }
        };
        input.click();
      });
    }
  }

  // ── Handlers ──────────────────────────────────

  async function handleExport() {
    if (!activeBlockId) return;
    exporting = true;
    exportStatus = '';
    try {
      const includeKeys = selectedKeys.size > 0 ? Array.from(selectedKeys) : undefined;
      const data = await api.importExport.export(activeBlockId, includeKeys);
      const nodeCount = data.nodes?.length ?? 0;
      const edgeCount = data.edges?.length ?? 0;
      const name = activePath.replace(/::/g, '_') || 'export';
      await saveJsonFile(data, `${name}.json`);
      exportStatus = `Exported ${nodeCount} nodes, ${edgeCount} edges`;
    } catch (err: any) {
      exportStatus = `Error: ${err.message}`;
    } finally {
      exporting = false;
    }
  }

  async function handleExportZip() {
    if (!activeBlockId) return;
    exporting = true;
    exportStatus = '';
    try {
      const blob = await api.importExport.exportZip(activeBlockId);
      const name = activePath.replace(/::/g, '_') || 'export';
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = `${name}.zip`;
      a.click();
      URL.revokeObjectURL(a.href);
      exportStatus = 'Exported ZIP with files';
    } catch (err: any) {
      exportStatus = `Error: ${err.message}`;
    } finally {
      exporting = false;
    }
  }

  async function handleChooseFile() {
    try {
      selectedData = await openImportFile();
      importStatus = '';
    } catch {
      console.warn('File selection cancelled or failed');
      selectedData = null;
      selectedZipFile = null;
      selectedFileName = '';
    }
  }

  async function handleImport() {
    // ZIP import
    if (selectedZipFile) {
      importing = true;
      importStatus = '';
      try {
        const parentId = importAtRoot ? undefined : activeBlockId ?? undefined;
        const result = await api.importExport.importZip(selectedZipFile, parentId, importMode);
        const created = result.created ?? 0;
        const skipped = result.skipped ?? 0;
        importStatus = `Created ${created}, Skipped ${skipped} (from ZIP)`;
        if (importAtRoot) {
          await blockTree.loadRoots();
        } else if (activeBlockId) {
          await blockTree.loadChildrenWithContent(activeBlockId, true);
        }
        selectedZipFile = null;
        selectedFileName = '';
      } catch (err: any) {
        importStatus = `Error: ${err.message}`;
      } finally {
        importing = false;
      }
      return;
    }

    // JSON import
    if ((!activeBlockId && !importAtRoot) || !selectedData) return;
    importing = true;
    importStatus = '';
    try {
      const result = importAtRoot
        ? await api.importExport.importAtRoot(
            selectedData,
            importMode,
            matchStrategy || undefined,
            importMode === 'merge' ? globalLink : undefined,
          )
        : await api.importExport.import(
            activeBlockId!,
            selectedData,
            importMode,
            matchStrategy || undefined,
            importMode === 'merge' ? globalLink : undefined,
          );
      const created = result.created ?? result.nodes_created ?? 0;
      const skipped = result.skipped ?? result.nodes_skipped ?? 0;
      const linked = result.linked ?? 0;
      let status = `Created ${created}, Skipped ${skipped}`;
      if (linked > 0) status += `, Linked ${linked}`;
      importStatus = status;
      // Refresh tree
      if (importAtRoot) {
        await blockTree.loadRoots();
      } else if (activeBlockId) {
        await blockTree.loadChildrenWithContent(activeBlockId, true);
      }
      // Reset file selection
      selectedData = null;
      selectedFileName = '';
    } catch (err: any) {
      importStatus = `Error: ${err.message}`;
    } finally {
      importing = false;
    }
  }
</script>

<div class="ie-panel">
  <div class="dl-header">
    <span class="dl-title">Import / Export</span>
  </div>

  <div class="ie-content">
    <!-- Export Section -->
    <section class="ie-section">
      <h3 class="ie-section-title">Export</h3>
      {#if activeBlockId}
        <div class="ie-path">{activePath}</div>
      {:else}
        <div class="ie-hint">Navigate to a block to enable export</div>
      {/if}

      {#if propertyKeys.length > 0}
        <div class="ie-keys">
          <span class="ie-label">Include properties:</span>
          <div class="ie-key-list">
            {#each propertyKeys.filter(k => !k.startsWith('_')) as key}
              <label class="ie-key-check">
                <input
                  type="checkbox"
                  checked={key === 'name' || selectedKeys.has(key)}
                  disabled={key === 'name'}
                  onchange={() => toggleKey(key)}
                />
                {key}
              </label>
            {/each}
            {#if propertyKeys.some(k => k.startsWith('_'))}
              <div class="ie-key-divider">internal</div>
              {#each propertyKeys.filter(k => k.startsWith('_')) as key}
                <label class="ie-key-check ie-key-internal">
                  <input
                    type="checkbox"
                    checked={selectedKeys.has(key)}
                    onchange={() => toggleKey(key)}
                  />
                  {key}
                </label>
              {/each}
            {/if}
          </div>
        </div>
      {/if}

      <div class="ie-btn-row">
        <button
          class="ie-btn"
          disabled={!activeBlockId || exporting}
          onclick={handleExport}
        >
          {exporting ? 'Exporting...' : 'Export JSON'}
        </button>
        <button
          class="ie-btn ie-btn-secondary"
          disabled={!activeBlockId || exporting}
          onclick={handleExportZip}
          title="Export with media files as ZIP"
        >
          {exporting ? 'Exporting...' : 'Export ZIP'}
        </button>
      </div>
      {#if exportStatus}
        <div class="ie-status" class:ie-error={exportStatus.startsWith('Error')}>{exportStatus}</div>
      {/if}
    </section>

    <!-- Import Section -->
    <section class="ie-section">
      <h3 class="ie-section-title">Import</h3>
      <button class="ie-btn ie-btn-secondary" onclick={handleChooseFile}>
        Choose file...
      </button>
      {#if selectedFileName}
        <div class="ie-file">{selectedFileName}</div>
      {/if}

      <div class="ie-mode">
        <label class="ie-radio">
          <input type="radio" bind:group={importMode} value="merge" />
          Merge
        </label>
        <label class="ie-radio">
          <input type="radio" bind:group={importMode} value="copy" />
          Copy
        </label>
      </div>
      <div class="ie-hint">Merge: skip duplicates. Copy: create fresh.</div>

      {#if importMode === 'merge'}
        <div class="ie-match">
          <label class="ie-label" for="match-strategy">Match by:</label>
          <select id="match-strategy" class="ie-select" bind:value={matchStrategy}>
            <option value="">Auto</option>
            <option value="content_identity">Content only</option>
            <option value="merkle">Content + structure</option>
            <option value="topology">Full topology</option>
            <option value="export_hash">Legacy (v1 compat)</option>
          </select>
        </div>

        <label class="ie-check-option">
          <input type="checkbox" bind:checked={globalLink} />
          Link to existing content globally
        </label>
        <div class="ie-hint">When enabled, searches the entire database for matching content to hard-link instead of creating new blocks.</div>
      {/if}

      <label class="ie-check-option">
        <input type="checkbox" bind:checked={importAtRoot} />
        Import at root level
      </label>
      {#if !importAtRoot && !activeBlockId}
        <div class="ie-hint">Navigate to a block or enable root import</div>
      {/if}

      <button
        class="ie-btn"
        disabled={(!activeBlockId && !importAtRoot) || (!selectedData && !selectedZipFile) || importing}
        onclick={handleImport}
      >
        {importing ? 'Importing...' : selectedZipFile ? 'Import ZIP' : 'Import'}
      </button>
      {#if importStatus}
        <div class="ie-status" class:ie-error={importStatus.startsWith('Error')}>{importStatus}</div>
      {/if}
    </section>
  </div>
</div>

<style>
  .ie-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
  }

  .ie-content {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
  }

  .ie-section {
    margin-bottom: 20px;
  }

  .ie-section-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin: 0 0 8px 0;
  }

  .ie-path {
    font-size: 12px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
    margin-bottom: 8px;
    word-break: break-all;
  }

  .ie-hint {
    font-size: 11px;
    color: var(--text-muted);
    margin-bottom: 8px;
  }

  .ie-btn {
    display: block;
    width: 100%;
    padding: 6px 12px;
    margin-bottom: 8px;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-size: 12px;
    cursor: pointer;
    transition: background 0.1s;
  }

  .ie-btn:hover:not(:disabled) {
    background: var(--bg-tertiary);
  }

  .ie-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .ie-btn-secondary {
    background: transparent;
  }

  .ie-btn-row {
    display: flex;
    gap: 6px;
  }

  .ie-btn-row .ie-btn {
    flex: 1;
    margin-bottom: 8px;
  }

  .ie-file {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--accent-color);
    margin-bottom: 8px;
    word-break: break-all;
  }

  .ie-mode {
    display: flex;
    gap: 16px;
    margin-bottom: 4px;
  }

  .ie-radio {
    font-size: 12px;
    color: var(--text-secondary);
    display: flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
  }

  .ie-match {
    margin-bottom: 8px;
  }

  .ie-label {
    font-size: 11px;
    color: var(--text-muted);
    display: block;
    margin-bottom: 4px;
  }

  .ie-select {
    width: 100%;
    padding: 4px 8px;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-size: 12px;
  }

  .ie-status {
    font-size: 11px;
    color: var(--text-secondary);
    padding: 4px 0;
  }

  .ie-error {
    color: #f7768e;
  }

  .ie-keys {
    margin-bottom: 8px;
  }

  .ie-key-list {
    max-height: 150px;
    overflow-y: auto;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    padding: 4px;
    margin-top: 4px;
  }

  .ie-key-check {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
    padding: 2px 4px;
    cursor: pointer;
  }

  .ie-key-check:hover {
    background: var(--bg-secondary);
  }

  .ie-key-internal {
    color: var(--text-muted);
  }

  .ie-key-divider {
    font-size: 10px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 4px 4px 2px;
    border-top: 1px solid var(--border-color);
    margin-top: 4px;
  }

  .ie-check-option {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 4px;
    cursor: pointer;
  }
</style>
