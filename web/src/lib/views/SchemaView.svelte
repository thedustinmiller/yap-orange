<script lang="ts">
  import type { TreeNode } from '../blockTree.svelte'
  import { blockTree } from '../blockTree.svelte'
  import { api } from '../api'
  import type { SchemaField } from '../types'
  import { enterNavigationMode } from '../appState.svelte'

  let { node, isEditing = false }: { node: TreeNode; isEditing?: boolean } = $props()
  let containerEl: HTMLDivElement | undefined = $state()

  const FIELD_TYPES = ['string', 'number', 'boolean', 'date', 'enum', 'ref', 'text'] as const

  // Derive fields from node.properties, defaulting to empty array
  let fields = $state<SchemaField[]>([])

  $effect(() => {
    const raw = node.properties?.fields
    if (Array.isArray(raw)) {
      fields = raw as SchemaField[]
    } else {
      fields = []
    }
  })

  let saveTimer: ReturnType<typeof setTimeout> | null = null

  function scheduleFieldSave() {
    // Update the store-owned node for immediate UI reactivity
    const storeNode = blockTree.getNode(node.id)
    if (storeNode) {
      if (!storeNode.properties) storeNode.properties = {}
      storeNode.properties.fields = [...fields]
    }

    if (saveTimer !== null) clearTimeout(saveTimer)
    saveTimer = setTimeout(async () => {
      saveTimer = null
      try {
        if (!node.lineage_id) return
        await api.atoms.update(node.lineage_id, {
          content: node.content ?? '',
          properties: { ...(node.properties ?? {}), fields: [...fields] },
        })
      } catch (err) {
        console.error('Failed to save schema fields:', err)
      }
    }, 500)
  }

  function addField() {
    fields = [...fields, { name: '', type: 'string', required: false }]
    scheduleFieldSave()
  }

  function removeField(index: number) {
    fields = fields.filter((_, i) => i !== index)
    scheduleFieldSave()
  }

  function updateFieldName(index: number, name: string) {
    fields = fields.map((f, i) => (i === index ? { ...f, name } : f))
    scheduleFieldSave()
  }

  function updateFieldType(index: number, type: SchemaField['type']) {
    fields = fields.map((f, i) => {
      if (i !== index) return f
      const updated: SchemaField = { ...f, type }
      // Clear irrelevant keys when switching types
      if (type !== 'enum') delete updated.options
      if (type !== 'ref') delete updated.target_type
      return updated
    })
    scheduleFieldSave()
  }

  function updateFieldOptions(index: number, raw: string) {
    const options = raw
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
    fields = fields.map((f, i) => (i === index ? { ...f, options } : f))
    scheduleFieldSave()
  }

  function updateFieldTargetType(index: number, target_type: string) {
    fields = fields.map((f, i) => (i === index ? { ...f, target_type } : f))
    scheduleFieldSave()
  }

  function updateFieldRequired(index: number, required: boolean) {
    fields = fields.map((f, i) => (i === index ? { ...f, required } : f))
    scheduleFieldSave()
  }

  // Focus first input when entering edit mode
  $effect(() => {
    if (isEditing && containerEl) {
      queueMicrotask(() => {
        const first = containerEl?.querySelector('input, select') as HTMLElement | null
        first?.focus()
      })
    }
  })

  function handleKeydown(e: KeyboardEvent) {
    if (!isEditing) return
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      enterNavigationMode(node.id)
      return
    }
    if (e.key === 'Tab') {
      e.preventDefault()
      e.stopPropagation()
      const inputs = Array.from(
        containerEl?.querySelectorAll('input, select') ?? []
      ) as HTMLElement[]
      if (inputs.length === 0) return
      const idx = inputs.indexOf(document.activeElement as HTMLElement)
      const next = e.shiftKey
        ? inputs[(idx - 1 + inputs.length) % inputs.length]
        : inputs[(idx + 1) % inputs.length]
      next.focus()
    }
  }

  function editCapture(el: HTMLElement) {
    el.addEventListener('click', (e: MouseEvent) => { if (isEditing) e.stopPropagation() })
    el.addEventListener('keydown', (e: Event) => handleKeydown(e as KeyboardEvent))
  }
</script>

<div class="schema-view" bind:this={containerEl} use:editCapture>
  {#if !isEditing}
    <span class="schema-nav">
      {#each fields as field, i}
        {#if i > 0}<span class="schema-nav-sep"> · </span>{/if}
        <span class="schema-nav-field">{field.name}</span><span class="schema-nav-type">:{field.type}</span>
      {/each}
      {#if fields.length === 0}
        <span class="schema-nav-empty">no fields</span>
      {/if}
    </span>
  {:else}
  <div class="schema-header">
    <span class="schema-icon">⬡</span>
    <span class="schema-title">Schema: {node.name}</span>
    <span class="schema-count">{fields.length} {fields.length === 1 ? 'field' : 'fields'}</span>
  </div>

  {#if fields.length > 0}
    <div class="fields-list">
      <!-- Column headers -->
      <div class="field-headers">
        <span class="col-name">Name</span>
        <span class="col-type">Type</span>
        <span class="col-extra">Options / Target</span>
        <span class="col-req">Req</span>
        <span class="col-del"></span>
      </div>

      {#each fields as field, i}
        <div class="field-row">
          <input
            class="col-name field-input"
            type="text"
            value={field.name}
            placeholder="field_name"
            aria-label="Field name"
            oninput={(e) => updateFieldName(i, (e.target as HTMLInputElement).value)}
          />
          <select
            class="col-type field-select"
            value={field.type}
            aria-label="Field type"
            onchange={(e) => updateFieldType(i, (e.target as HTMLSelectElement).value as SchemaField['type'])}
          >
            {#each FIELD_TYPES as t}
              <option value={t}>{t}</option>
            {/each}
          </select>
          <div class="col-extra">
            {#if field.type === 'enum'}
              <input
                class="field-input"
                type="text"
                value={(field.options ?? []).join(', ')}
                placeholder="opt1, opt2, opt3"
                aria-label="Enum options (comma-separated)"
                oninput={(e) => updateFieldOptions(i, (e.target as HTMLInputElement).value)}
              />
            {:else if field.type === 'ref'}
              <input
                class="field-input"
                type="text"
                value={field.target_type ?? ''}
                placeholder="type name"
                aria-label="Target type name"
                oninput={(e) => updateFieldTargetType(i, (e.target as HTMLInputElement).value)}
              />
            {:else}
              <span class="col-extra-empty">—</span>
            {/if}
          </div>
          <div class="col-req">
            <input
              type="checkbox"
              checked={field.required ?? false}
              onchange={(e) => updateFieldRequired(i, (e.target as HTMLInputElement).checked)}
              aria-label="Required"
            />
          </div>
          <button class="col-del" onclick={() => removeField(i)} aria-label="Remove field">×</button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="no-fields">No fields defined</div>
  {/if}

  <button class="add-field-btn" onclick={addField}>+ Add field</button>
  {/if}
</div>

<style>
  .schema-view {
    min-width: 0;
  }

  .schema-nav {
    font-size: 12px;
    color: var(--text-secondary, #888);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: block;
  }

  .schema-nav-field { color: var(--text-primary, #ccc); }
  .schema-nav-type { color: var(--text-muted, #555); font-size: 11px; }
  .schema-nav-sep { color: var(--text-muted, #444); }
  .schema-nav-empty { color: var(--text-muted, #555); font-style: italic; }

  .schema-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    margin-top: 4px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary, #888);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .schema-icon {
    font-size: 14px;
    opacity: 0.7;
  }

  .schema-title {
    flex: 1;
  }

  .schema-count {
    font-weight: 400;
    font-size: 11px;
    color: var(--text-muted, #555);
    text-transform: none;
    letter-spacing: 0;
  }

  .fields-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 8px;
  }

  .field-headers {
    display: grid;
    grid-template-columns: 1fr 90px 1fr 32px 20px;
    gap: 4px;
    font-size: 10px;
    color: var(--text-muted, #555);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 0 2px 4px;
    border-bottom: 1px solid var(--border-color, #333);
    margin-bottom: 2px;
  }

  .field-row {
    display: grid;
    grid-template-columns: 1fr 90px 1fr 32px 20px;
    gap: 4px;
    align-items: center;
    padding: 2px 0;
  }

  .field-input {
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 3px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    padding: 2px 5px;
    outline: none;
    font-family: inherit;
    width: 100%;
    box-sizing: border-box;
  }

  .field-input:focus {
    border-color: var(--accent-color, #7aa2f7);
  }

  .field-select {
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 3px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    padding: 2px 4px;
    outline: none;
    font-family: inherit;
    width: 100%;
    box-sizing: border-box;
  }

  .field-select:focus {
    border-color: var(--accent-color, #7aa2f7);
  }

  .col-extra-empty {
    color: var(--text-muted, #555);
    font-size: 11px;
    padding-left: 4px;
  }

  .col-req {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .col-req input[type="checkbox"] {
    cursor: pointer;
    accent-color: var(--accent-color, #7aa2f7);
  }

  .col-del {
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: var(--text-muted, #555);
    font-size: 14px;
    line-height: 1;
    border-radius: 3px;
    transition: color 0.1s;
    background: transparent;
    border: none;
    padding: 0;
    font-family: inherit;
  }

  .col-del:hover {
    color: #f7768e;
  }

  .no-fields {
    font-size: 12px;
    color: var(--text-muted, #555);
    font-style: italic;
    padding: 4px 0 8px;
  }

  .add-field-btn {
    background: transparent;
    border: 1px dashed var(--border-color, #333);
    border-radius: 4px;
    color: var(--text-secondary, #888);
    font-size: 12px;
    padding: 4px 12px;
    cursor: pointer;
    font-family: inherit;
    transition: border-color 0.1s, color 0.1s;
    width: 100%;
    text-align: left;
  }

  .add-field-btn:hover {
    border-color: var(--accent-color, #7aa2f7);
    color: var(--accent-color, #7aa2f7);
  }
</style>
