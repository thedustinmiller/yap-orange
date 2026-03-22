<script lang="ts">
  import type { SchemaField } from '../../types'

  let { value, field, onChange }: { value: unknown; field: SchemaField; onChange: (v: number | null) => void } = $props()
</script>

<input
  class="field-input"
  type="number"
  value={value as number ?? ''}
  placeholder={field.name}
  aria-label={field.name}
  oninput={(e) => {
    const v = (e.target as HTMLInputElement).value
    onChange(v === '' ? null : parseFloat(v))
  }}
/>

<style>
  .field-input {
    width: 100%;
    max-width: 120px;
    box-sizing: border-box;
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 3px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    padding: 3px 6px;
    outline: none;
    font-family: inherit;
  }
  .field-input:focus {
    border-color: var(--accent-color, #7aa2f7);
  }
</style>
