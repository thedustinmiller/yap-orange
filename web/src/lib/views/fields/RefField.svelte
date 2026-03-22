<script lang="ts">
  import type { SchemaField } from '../../types'
  import { api } from '../../api'

  let { value, field, onChange }: { value: unknown; field: SchemaField; onChange: (v: string) => void } = $props()

  interface RefOption {
    lineage_id: string
    name: string
  }

  let options = $state<RefOption[]>([])
  let loading = $state(false)

  $effect(() => {
    if (field.target_type) {
      loading = true
      api.blocks.list({ content_type: field.target_type }).then((blocks) => {
        options = blocks.map((b) => ({ lineage_id: b.lineage_id, name: b.name }))
        loading = false
      }).catch(() => { loading = false })
    }
  })
</script>

<select
  class="field-select"
  value={value as string ?? ''}
  aria-label={field.name}
  disabled={loading}
  onchange={(e) => onChange((e.target as HTMLSelectElement).value)}
>
  <option value="">{loading ? 'Loading…' : `Select ${field.target_type ?? 'entry'}…`}</option>
  {#each options as opt}
    <option value={opt.lineage_id}>{opt.name}</option>
  {/each}
</select>

<style>
  .field-select {
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 3px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    padding: 3px 6px;
    outline: none;
    font-family: inherit;
  }
  .field-select:focus {
    border-color: var(--accent-color, #7aa2f7);
  }
  .field-select:disabled {
    opacity: 0.5;
  }
</style>
