<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, keymap } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { json } from '@codemirror/lang-json';
  import { appState } from './appState.svelte';
  import { blockTree } from './blockTree.svelte';
  import { api } from './api';
  import { navigateTo } from './appState.svelte';
  import { yapTheme } from './editor/theme';
  import { addToast } from './toastStore.svelte';

  let containerEl: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;

  // Track which block is loaded in the editor so saves go to the right place
  let _editingLineageId: string | null = null;
  let _editingBlockId: string | null = null;

  // UI state (reactive so header updates)
  let parseError = $state(false);
  let saving = $state(false);
  let dirty = $state(false);

  // Set before programmatic editor updates to prevent marking dirty
  let _externalUpdate = false;

  // ── Active block resolution ──────────────────────────────────────────────

  interface ActiveInfo {
    lineageId: string;
    blockId: string;
    content: string;
    properties: Record<string, unknown>;
    contentType: string | null;
  }

  function getActiveInfo(): ActiveInfo | null {
    if (appState.selectedBlockIds.length > 0) {
      const node = blockTree.getNode(appState.selectedBlockIds[0]);
      if (node?.properties && Object.keys(node.properties).length > 0) {
        return {
          lineageId: node.lineage_id,
          blockId: node.id,
          content: node.content ?? '',
          properties: node.properties,
          contentType: node.content_type ?? null,
        };
      }
      // Even if no properties, still return info for content_type display
      if (node) {
        return {
          lineageId: node.lineage_id,
          blockId: node.id,
          content: node.content ?? '',
          properties: {},
          contentType: node.content_type ?? null,
        };
      }
    }
    if (appState.currentBlock) {
      const cb = appState.currentBlock;
      return {
        lineageId: cb.lineage_id,
        blockId: cb.id,
        content: cb.content,
        properties: cb.properties && Object.keys(cb.properties).length > 0 ? cb.properties : {},
        contentType: cb.content_type ?? null,
      };
    }
    return null;
  }

  function formatJson(props: Record<string, unknown> | null): string {
    if (!props) return '';
    return JSON.stringify(props, null, 2);
  }

  // ── Save ─────────────────────────────────────────────────────────────────

  async function save() {
    if (!dirty || !_editingLineageId || !view) return;
    const lineageId = _editingLineageId;
    const blockId = _editingBlockId;
    const text = view.state.doc.toString();
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(text);
    } catch {
      parseError = true;
      return; // Don't save invalid JSON
    }
    parseError = false;
    saving = true;
    try {
      // Get current rendered content so we don't clobber it
      const node = blockId ? blockTree.getNode(blockId) : null;
      const content = node?.content ?? appState.currentBlock?.content ?? '';
      await api.atoms.update(lineageId, { content, properties: parsed });
      // Only patch if we're still on the same block
      if (_editingBlockId === blockId && node) node.properties = parsed;
      if (_editingLineageId === lineageId) dirty = false;
      // If the name property changed, reload the block so tree/namespace stay in sync
      if (blockId && parsed.name !== undefined) {
        await blockTree.loadContent(blockId, true);
        if (appState.activeNamespaceBlockId === blockId) {
          await navigateTo(blockId);
        }
      }
    } catch (err) {
      console.error('Failed to save properties:', err);
      addToast('Failed to save properties', 'error');
    } finally {
      saving = false;
    }
  }

  // ── Sync editor when active block changes ────────────────────────────────

  $effect(() => {
    const info = getActiveInfo();
    if (!view) return;

    const newText = formatJson(info?.properties ?? null);
    const current = view.state.doc.toString();

    // Same block, same content — nothing to do
    if (info?.blockId === _editingBlockId && current === newText) return;

    // Flush pending save before switching blocks
    if (info?.blockId !== _editingBlockId && dirty) {
      save(); // fire-and-forget flush of previous block's changes
    }
    _editingLineageId = info?.lineageId ?? null;
    _editingBlockId = info?.blockId ?? null;
    dirty = false;
    parseError = false;

    if (current === newText) return;

    // Update editor without marking dirty
    _externalUpdate = true;
    view.dispatch({ changes: { from: 0, to: current.length, insert: newText } });
    _externalUpdate = false;
  });

  // ── CM6 setup ────────────────────────────────────────────────────────────

  onMount(() => {
    if (!containerEl) return;

    const info = getActiveInfo();
    _editingLineageId = info?.lineageId ?? null;
    _editingBlockId = info?.blockId ?? null;

    view = new EditorView({
      state: EditorState.create({
        doc: formatJson(info?.properties ?? null),
        extensions: [
          json(),
          yapTheme,
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          EditorView.theme({
            '&': { height: '100%', fontSize: '12px' },
            '.cm-scroller': { overflow: 'auto', fontFamily: 'var(--font-mono)' },
          }),
          EditorView.contentAttributes.of({ 'aria-label': 'Properties JSON editor' }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged && !_externalUpdate) {
              dirty = true;
              // Validate JSON while typing — drive the error indicator
              try {
                JSON.parse(update.view.state.doc.toString());
                parseError = false;
              } catch {
                parseError = true;
              }
            }
            // Blur → save
            if (update.focusChanged && !update.view.hasFocus) {
              save();
            }
          }),
        ],
      }),
      parent: containerEl,
    });
  });

  onDestroy(() => {
    view?.destroy();
  });

  // ── Derived display ──────────────────────────────────────────────────────

  function getDisplayProperties(): Record<string, unknown> | null {
    const info = getActiveInfo();
    if (!info) return null;
    return Object.keys(info.properties).length > 0 ? info.properties : null;
  }

  function getContentType(): string | null {
    const info = getActiveInfo();
    if (!info) return null;
    const ct = info.contentType;
    return ct && ct !== '' ? ct : null;
  }
</script>

<div class="properties-panel">
  <div class="props-header">
    <span class="props-title">Properties</span>
    {#if getDisplayProperties()}
      <span class="props-count">{Object.keys(getDisplayProperties()!).length}</span>
    {/if}
    <span class="props-status">
      {#if saving}
        <span class="status-saving">saving…</span>
      {:else if parseError}
        <span class="status-error">invalid JSON</span>
      {:else if dirty}
        <span class="status-dirty">●</span>
      {/if}
    </span>
  </div>

  {#if getContentType()}
    <div class="content-type-row">
      <span class="content-type-label">Type</span>
      <span class="content-type-value">{getContentType()}</span>
    </div>
  {/if}

  <div class="props-content">
    {#if !getDisplayProperties()}
      <div class="props-empty">
        {appState.selectedBlockIds.length > 0 || appState.currentBlock
          ? 'No properties on this block'
          : 'Select a block to see properties'}
      </div>
    {/if}
    <!-- Always in DOM so onMount can initialize CM6 -->
    <div
      class="props-editor"
      class:props-editor-hidden={!getDisplayProperties()}
      class:props-editor-error={parseError}
      bind:this={containerEl}
    ></div>
  </div>
</div>

<style>
  .properties-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
  }

  .props-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
  }

  .props-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .props-count {
    font-size: 10px;
    background: var(--accent-color);
    color: white;
    padding: 0 6px;
    border-radius: 8px;
  }

  .props-status {
    margin-left: auto;
    font-size: 10px;
  }

  .status-saving {
    color: var(--text-muted);
    font-style: italic;
  }

  .status-error {
    color: #f7768e;
    font-weight: 600;
  }

  .status-dirty {
    color: var(--accent-color);
  }

  .content-type-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-color);
    flex-shrink: 0;
  }

  .content-type-label {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .content-type-value {
    font-size: 12px;
    font-family: var(--font-mono, monospace);
    color: var(--accent-color);
  }

  .props-content {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .props-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 80px;
    color: var(--text-muted);
    font-size: 12px;
  }

  .props-editor {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .props-editor-hidden {
    display: none;
  }

  .props-editor-error :global(.cm-editor) {
    outline: 1px solid #f7768e;
  }

  .props-editor :global(.cm-editor) {
    height: 100%;
    flex: 1;
  }
</style>
