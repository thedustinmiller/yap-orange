<script lang="ts">
  import type { TreeNode } from '../blockTree.svelte'
  import { blockTree } from '../blockTree.svelte'
  import { api } from '../api'
  import { enterNavigationMode } from '../appState.svelte'

  let { node, isEditing = false }: { node: TreeNode; isEditing?: boolean } = $props()

  interface TimeRange {
    start: string
    end?: string
  }

  let status = $derived<'todo' | 'doing' | 'done'>(
    (node.properties?.status as 'todo' | 'doing' | 'done') ?? 'todo'
  )
  let description = $derived<string>(
    (node.properties?.description as string) ?? ''
  )
  let timeRanges = $derived<TimeRange[]>(
    Array.isArray(node.properties?.time_ranges)
      ? (node.properties!.time_ranges as TimeRange[])
      : []
  )

  let containerEl: HTMLDivElement | undefined = $state()

  // Live tick for "doing" state
  let now = $state(Date.now())
  $effect(() => {
    if (status === 'doing') {
      const id = setInterval(() => { now = Date.now() }, 1000)
      return () => clearInterval(id)
    }
  })

  // Focus textarea when entering edit mode
  $effect(() => {
    if (isEditing && containerEl) {
      queueMicrotask(() => {
        const ta = containerEl?.querySelector('textarea') as HTMLElement | null
        ta?.focus()
      })
    }
  })

  let totalTimeMs = $derived.by(() => {
    let total = 0
    for (const range of timeRanges) {
      const start = new Date(range.start).getTime()
      const end = range.end ? new Date(range.end).getTime() : now
      if (!isNaN(start) && !isNaN(end) && end > start) total += end - start
    }
    return total
  })

  function formatDuration(ms: number): string {
    if (ms < 60_000) return '<1m'
    const totalMinutes = Math.floor(ms / 60_000)
    const days = Math.floor(totalMinutes / (60 * 24))
    const hours = Math.floor((totalMinutes % (60 * 24)) / 60)
    const minutes = totalMinutes % 60
    const parts: string[] = []
    if (days > 0) parts.push(`${days}d`)
    if (hours > 0) parts.push(`${hours}h`)
    if (minutes > 0) parts.push(`${minutes}m`)
    return parts.join('') || '0m'
  }

  let saveTimer: ReturnType<typeof setTimeout> | null = null

  function saveProperties(props: Record<string, unknown>) {
    const storeNode = blockTree.getNode(node.id)
    if (storeNode) {
      if (!storeNode.properties) storeNode.properties = {}
      Object.assign(storeNode.properties, props)
    }

    if (saveTimer !== null) clearTimeout(saveTimer)
    saveTimer = setTimeout(async () => {
      saveTimer = null
      try {
        if (!node.lineage_id) return
        await api.atoms.update(node.lineage_id, {
          content: node.content ?? '',
          properties: { ...(node.properties ?? {}), ...props },
        })
      } catch (err) {
        console.error('Failed to save todo:', err)
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
      }).catch((err) => console.error('Failed to save todo:', err))
    }
  }

  function handleCheckboxClick(e: MouseEvent) {
    e.stopPropagation()
    if (status === 'doing') {
      const ranges = [...timeRanges]
      if (ranges.length > 0 && !ranges[ranges.length - 1].end) {
        ranges[ranges.length - 1] = { ...ranges[ranges.length - 1], end: new Date().toISOString() }
      }
      saveProperties({ status: 'done', time_ranges: ranges })
    } else if (status === 'done') {
      saveProperties({ status: 'todo' })
    } else {
      handleLabelClick(e)
    }
  }

  function handleLabelClick(e: MouseEvent) {
    e.stopPropagation()
    if (status === 'todo') {
      saveProperties({ status: 'doing', time_ranges: [...timeRanges, { start: new Date().toISOString() }] })
    } else if (status === 'doing') {
      const ranges = [...timeRanges]
      if (ranges.length > 0 && !ranges[ranges.length - 1].end) {
        ranges[ranges.length - 1] = { ...ranges[ranges.length - 1], end: new Date().toISOString() }
      }
      saveProperties({ status: 'todo', time_ranges: ranges })
    } else if (status === 'done') {
      saveProperties({ status: 'todo' })
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!isEditing) return
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      flushSave()
      enterNavigationMode(node.id)
    }
  }

  function editCapture(el: HTMLElement) {
    el.addEventListener('click', (e: MouseEvent) => { if (isEditing) e.stopPropagation() })
    el.addEventListener('keydown', (e: Event) => handleKeydown(e as KeyboardEvent))
  }
</script>

<div
  class="todo-view"
  bind:this={containerEl}
  use:editCapture
  role="presentation"
>
  <!-- Status row — always visible in both modes -->
  <div class="todo-row">
    <button
      class="todo-cb"
      class:checked={status === 'done'}
      onclick={handleCheckboxClick}
      role="checkbox"
      aria-checked={status === 'done'}
      aria-label={status === 'done' ? 'Mark as todo' : 'Mark as done'}
    >
      {#if status === 'done'}&#x2713;{/if}
    </button>

    <button
      class="todo-status"
      class:doing={status === 'doing'}
      class:done={status === 'done'}
      onclick={handleLabelClick}
      aria-label="Cycle status: {status}"
    >{status.toUpperCase()}</button>

    {#if description && !isEditing}
      <span class="todo-desc-inline" class:done={status === 'done'}>{description}</span>
    {/if}

    {#if totalTimeMs > 0 || status === 'doing'}
      <span class="todo-time" class:doing={status === 'doing'}>{formatDuration(totalTimeMs)}</span>
    {/if}
  </div>

  <!-- Description editor — edit mode only -->
  {#if isEditing}
    <div class="todo-edit">
      <label class="todo-edit-label" for="todo-desc">Description</label>
      <textarea
        id="todo-desc"
        class="todo-edit-textarea"
        value={description}
        placeholder="Add a description…"
        aria-label="Todo description"
        rows="2"
        oninput={(e) => saveProperties({ description: (e.target as HTMLTextAreaElement).value })}
      ></textarea>
    </div>
  {/if}
</div>

<style>
  .todo-view {
    min-width: 0;
  }

  /* ── Status row — compact, scannable ── */
  .todo-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 20px;
  }

  .todo-cb {
    width: 14px;
    height: 14px;
    border: 1.5px solid var(--text-muted, #555);
    border-radius: 3px;
    background: transparent;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 700;
    color: white;
    line-height: 1;
    font-family: inherit;
    transition: border-color 0.15s, background 0.15s;
  }

  .todo-cb:hover { border-color: var(--accent-color, #7aa2f7); }
  .todo-cb.checked {
    background: var(--accent-color, #7aa2f7);
    border-color: var(--accent-color, #7aa2f7);
  }

  .todo-status {
    background: none;
    border: none;
    padding: 0 3px;
    font-family: inherit;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--text-muted, #555);
    cursor: pointer;
    border-radius: 2px;
    transition: color 0.1s;
    flex-shrink: 0;
  }

  .todo-status:hover { color: var(--text-primary, #ccc); }
  .todo-status.doing { color: var(--accent-color, #7aa2f7); }
  .todo-status.done { color: var(--text-muted, #555); }

  .todo-desc-inline {
    font-size: 12px;
    color: var(--text-secondary, #888);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  .todo-desc-inline.done {
    text-decoration: line-through;
    opacity: 0.6;
  }

  .todo-time {
    font-size: 10px;
    color: var(--text-muted, #555);
    font-variant-numeric: tabular-nums;
    margin-left: auto;
    flex-shrink: 0;
  }

  .todo-time.doing { color: var(--accent-color, #7aa2f7); }

  /* ── Edit mode: description field ── */
  .todo-edit {
    margin-top: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .todo-edit-label {
    font-size: 11px;
    color: var(--text-muted, #555);
    user-select: none;
  }

  .todo-edit-textarea {
    width: 100%;
    box-sizing: border-box;
    padding: 4px 6px;
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 3px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    font-family: inherit;
    resize: vertical;
    min-height: 36px;
    outline: none;
  }

  .todo-edit-textarea:focus {
    border-color: var(--accent-color, #7aa2f7);
  }
</style>
