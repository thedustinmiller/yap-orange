<script lang="ts">
  import type { TreeNode } from '../blockTree.svelte'
  import { blockTree } from '../blockTree.svelte'
  import { api } from '../api'
  import { navigateTo } from '../appState.svelte'
  import { getViewDefinition } from './typeViewRegistry'

  let { node, isEditing = false }: { node: TreeNode; isEditing?: boolean } = $props()

  let newTypeName = $state('')
  let creating = $state(false)
  let error = $state('')

  // Schema children — blocks under this types:: namespace with content_type="schema"
  let schemaChildren = $derived(
    node.children.filter(c => c.content_type === 'schema')
  )

  async function handleCreate() {
    const name = newTypeName.trim()
    if (!name) return
    if (creating) return

    // Validate: no special characters, no spaces
    if (!/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(name)) {
      error = 'Type names must start with a letter/underscore and contain only letters, numbers, underscores'
      return
    }

    // Check for duplicate
    const existing = schemaChildren.find(c => c.name === name)
    if (existing) {
      error = `Type "${name}" already exists`
      return
    }

    creating = true
    error = ''

    try {
      await api.blocks.create({
        namespace: node.namespace,
        name,
        content: '',
        content_type: 'schema',
        properties: { fields: [] },
      })

      newTypeName = ''

      // Reload children so the new type appears
      await blockTree.loadChildrenWithContent(node.id, true)
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to create type'
    } finally {
      creating = false
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault()
      e.stopPropagation()
      handleCreate()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      newTypeName = ''
      error = ''
    }
  }

  function handleTypeClick(child: TreeNode) {
    navigateTo(child.id)
  }

  function fieldSummary(child: TreeNode): string {
    const fields = child.properties?.fields
    if (!Array.isArray(fields) || fields.length === 0) return 'no fields'
    const names = fields
      .map((f: any) => f.name)
      .filter(Boolean)
      .slice(0, 4)
    const suffix = fields.length > 4 ? `, +${fields.length - 4}` : ''
    return names.join(', ') + suffix
  }
</script>

<div class="type-registry-view" role="presentation" onclick={(e) => e.stopPropagation()}>
  <div class="registry-header">
    <span class="registry-icon">{getViewDefinition('type_registry')?.icon ?? '⬡'}</span>
    <span class="registry-title">Types</span>
    <span class="registry-count">{schemaChildren.length} {schemaChildren.length === 1 ? 'type' : 'types'}</span>
  </div>

  <!-- Type list -->
  {#if schemaChildren.length > 0}
    <div class="type-list">
      {#each schemaChildren as child (child.id)}
        <button class="type-item" onclick={() => handleTypeClick(child)}>
          <span class="type-icon">{getViewDefinition(child.content_type)?.icon ?? '⬡'}</span>
          <span class="type-name">{child.name}</span>
          <span class="type-fields">{fieldSummary(child)}</span>
        </button>
      {/each}
    </div>
  {:else}
    <div class="no-types">No types defined yet</div>
  {/if}

  <!-- Create form -->
  <div class="create-form">
    <input
      class="create-input"
      type="text"
      bind:value={newTypeName}
      onkeydown={handleKeydown}
      placeholder="New type name..."
      disabled={creating}
    />
    <button
      class="create-btn"
      onclick={handleCreate}
      disabled={creating || !newTypeName.trim()}
    >
      {creating ? '...' : '+ Create'}
    </button>
  </div>

  {#if error}
    <div class="create-error">{error}</div>
  {/if}
</div>

<style>
  .type-registry-view {
    padding: 8px 10px;
    min-width: 0;
  }

  .registry-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .registry-icon {
    font-size: 14px;
    opacity: 0.7;
  }

  .registry-title {
    flex: 1;
  }

  .registry-count {
    font-weight: 400;
    font-size: 11px;
    color: var(--text-muted, #555);
    text-transform: none;
    letter-spacing: 0;
  }

  .type-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 10px;
  }

  .type-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-radius: 4px;
    cursor: pointer;
    transition: background 0.08s;
    font-size: 13px;
    background: none;
    border: none;
    width: 100%;
    text-align: left;
    color: inherit;
    font-family: inherit;
  }

  .type-item:hover {
    background: var(--bg-hover, rgba(255,255,255,0.05));
  }

  .type-icon {
    font-size: 12px;
    opacity: 0.6;
    flex-shrink: 0;
  }

  .type-name {
    color: var(--text-primary, #ccc);
    font-weight: 500;
  }

  .type-fields {
    color: var(--text-muted, #555);
    font-size: 11px;
    margin-left: auto;
    font-family: var(--font-mono, monospace);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .no-types {
    font-size: 12px;
    color: var(--text-muted, #555);
    font-style: italic;
    padding: 4px 0 10px;
  }

  .create-form {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .create-input {
    flex: 1;
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 4px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    padding: 4px 8px;
    outline: none;
    font-family: inherit;
  }

  .create-input:focus {
    border-color: var(--accent-color, #7aa2f7);
  }

  .create-input:disabled {
    opacity: 0.5;
  }

  .create-btn {
    background: transparent;
    border: 1px dashed var(--border-color, #333);
    border-radius: 4px;
    color: var(--text-secondary, #888);
    font-size: 12px;
    padding: 4px 12px;
    cursor: pointer;
    font-family: inherit;
    transition: border-color 0.1s, color 0.1s;
    white-space: nowrap;
  }

  .create-btn:hover:not(:disabled) {
    border-color: var(--accent-color, #7aa2f7);
    color: var(--accent-color, #7aa2f7);
  }

  .create-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .create-error {
    margin-top: 4px;
    font-size: 11px;
    color: #f7768e;
  }
</style>
