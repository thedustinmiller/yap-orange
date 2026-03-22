<script lang="ts">
  import type { TreeNode } from '../blockTree.svelte'
  import { blockTree } from '../blockTree.svelte'
  import type { SchemaField } from '../types'
  import { api } from '../api'
  import { resolveSchema } from '../schemaStore.svelte'
  import { enterNavigationMode } from '../appState.svelte'
  import type { Component } from 'svelte'
  import StringField from './fields/StringField.svelte'
  import NumberField from './fields/NumberField.svelte'
  import BooleanField from './fields/BooleanField.svelte'
  import DateField from './fields/DateField.svelte'
  import EnumField from './fields/EnumField.svelte'
  import TextField from './fields/TextField.svelte'
  import RefField from './fields/RefField.svelte'

  let { node, isEditing = false }: { node: TreeNode; isEditing?: boolean } = $props()

  let fields = $state<SchemaField[]>([])
  let schemaLoaded = $state(false)
  let containerEl: HTMLDivElement | undefined = $state()

  $effect(() => {
    const ct = node.content_type
    const pinnedAtomId = node.properties?._schema_atom_id as string | undefined
    if (!ct) return

    if (pinnedAtomId) {
      api.atoms.snapshot(pinnedAtomId).then((atom) => {
        const raw = atom.properties?.fields
        fields = Array.isArray(raw) ? (raw as SchemaField[]) : []
        schemaLoaded = true
      }).catch(() => {
        resolveSchema(ct, node.namespace).then((schema) => {
          fields = schema?.fields ?? []
          schemaLoaded = true
        })
      })
    } else {
      resolveSchema(ct, node.namespace).then((schema) => {
        fields = schema?.fields ?? []
        schemaLoaded = true
      })
    }
  })

  // Focus first input when entering edit mode
  $effect(() => {
    if (isEditing && containerEl) {
      queueMicrotask(() => {
        const first = containerEl?.querySelector('input, select, textarea') as HTMLElement | null
        first?.focus()
      })
    }
  })

  function getFieldValue(fieldName: string): unknown {
    return node.properties?.[fieldName] ?? undefined
  }

  let saveTimer: ReturnType<typeof setTimeout> | null = null

  function handleFieldChange(fieldName: string, value: unknown) {
    const storeNode = blockTree.getNode(node.id)
    if (storeNode) {
      if (!storeNode.properties) storeNode.properties = {}
      storeNode.properties[fieldName] = value
    }

    if (saveTimer !== null) clearTimeout(saveTimer)
    saveTimer = setTimeout(async () => {
      saveTimer = null
      try {
        if (!node.lineage_id) return
        await api.atoms.update(node.lineage_id, {
          content: node.content ?? '',
          properties: { ...(node.properties ?? {}) },
        })
      } catch (err) {
        console.error('Failed to save entry:', err)
      }
    }, 500)
  }

  function flushSave() {
    if (saveTimer !== null) {
      clearTimeout(saveTimer)
      saveTimer = null
      if (!node.lineage_id) return
      api.atoms.update(node.lineage_id, {
        content: node.content ?? '',
        properties: { ...(node.properties ?? {}) },
      }).catch((err) => console.error('Failed to save entry:', err))
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!isEditing) return

    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      flushSave()
      enterNavigationMode(node.id)
      return
    }

    if (e.key === 'Tab') {
      e.preventDefault()
      e.stopPropagation()
      const inputs = Array.from(
        containerEl?.querySelectorAll('input, select, textarea') ?? []
      ) as HTMLElement[]
      if (inputs.length === 0) return
      const idx = inputs.indexOf(document.activeElement as HTMLElement)
      const next = e.shiftKey
        ? inputs[(idx - 1 + inputs.length) % inputs.length]
        : inputs[(idx + 1) % inputs.length]
      next.focus()
    }
  }

  // Display-only fields (skip internal metadata)
  let displayFields = $derived(fields.filter(f => !f.name.startsWith('_') && f.name !== 'time_ranges'))

  function formatNav(field: SchemaField): string {
    const v = getFieldValue(field.name)
    if (v === undefined || v === null || v === '') return ''
    if (typeof v === 'boolean') return v ? 'yes' : 'no'
    const s = String(v)
    return s.length > 30 ? s.slice(0, 30) + '…' : s
  }

  const FIELD_COMPONENTS: Record<string, Component<any>> = {
    string: StringField,
    number: NumberField,
    boolean: BooleanField,
    date: DateField,
    enum: EnumField,
    text: TextField,
    ref: RefField,
  }

  function editCapture(el: HTMLElement) {
    el.addEventListener('click', (e: MouseEvent) => { if (isEditing) e.stopPropagation() })
    el.addEventListener('keydown', (e: Event) => handleKeydown(e as KeyboardEvent))
  }
</script>

<div
  class="entry-view"
  bind:this={containerEl}
  use:editCapture
  role="presentation"
>
  {#if !schemaLoaded}
    <span class="nav-hint">Loading…</span>
  {:else if isEditing}
    <div class="edit-form">
      {#each fields as field}
        <div class="edit-row">
          <label class="edit-label" for="field-{field.name}">
            {field.name}
            {#if field.required}<span class="edit-req">*</span>{/if}
          </label>
          <div class="edit-control" id="field-{field.name}">
            {#if FIELD_COMPONENTS[field.type]}
              {@const FieldComponent = FIELD_COMPONENTS[field.type]}
              <FieldComponent
                value={getFieldValue(field.name)}
                {field}
                onChange={(v: unknown) => handleFieldChange(field.name, v)}
              />
            {:else}
              <span class="edit-unknown">{field.type}</span>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <span class="nav-summary">
      {#each displayFields as field, i}
        {@const val = formatNav(field)}
        {#if val}
          {#if i > 0 && formatNav(displayFields[i-1])}<span class="nav-sep"> · </span>{/if}
          <span class="nav-val"><span class="nav-key">{field.name}:</span> {val}</span>
        {/if}
      {/each}
      {#if displayFields.every(f => !formatNav(f))}
        <span class="nav-empty">empty entry</span>
      {/if}
    </span>
  {/if}
</div>

<style>
  .entry-view {
    min-width: 0;
  }

  /* ── Nav mode: compact inline summary ── */
  .nav-summary {
    font-size: 12px;
    color: var(--text-secondary, #888);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: block;
  }

  .nav-key {
    color: var(--text-muted, #555);
  }

  .nav-sep {
    color: var(--text-muted, #444);
  }

  .nav-empty {
    color: var(--text-muted, #555);
    font-style: italic;
  }

  .nav-hint {
    font-size: 12px;
    color: var(--text-muted, #555);
    font-style: italic;
  }

  /* ── Edit mode: structured form ── */
  .edit-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 4px 0;
  }

  .edit-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
  }

  .edit-label {
    flex: 0 0 110px;
    font-size: 12px;
    color: var(--text-primary, #ccc);
    user-select: none;
  }

  .edit-req {
    color: var(--accent-color, #7aa2f7);
    margin-left: 2px;
  }

  .edit-control {
    flex: 1;
    min-width: 0;
  }

  .edit-unknown {
    font-size: 11px;
    color: var(--text-muted, #555);
    font-style: italic;
  }
</style>
