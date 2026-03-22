<script lang="ts">
  import type { SchemaField } from '../../types'

  let { value, field, onChange }: { value: unknown; field: SchemaField; onChange: (v: string) => void } = $props()

  let options = $derived(field.options ?? [])
</script>

<select
  class="field-select"
  value={value as string ?? ''}
  aria-label={field.name}
  onchange={(e) => onChange((e.target as HTMLSelectElement).value)}
>
  <option value="">—</option>
  {#each options as opt}
    <option value={opt}>{opt}</option>
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
</style>
