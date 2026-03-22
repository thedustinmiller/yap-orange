<script lang="ts">
  import type { SchemaField } from '../../types'

  let { value, field, onChange }: { value: unknown; field: SchemaField; onChange: (v: string) => void } = $props()
</script>

<input
  class="field-input"
  type="date"
  value={value as string ?? ''}
  aria-label={field.name}
  oninput={(e) => onChange((e.target as HTMLInputElement).value)}
/>

<style>
  .field-input {
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 3px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    padding: 3px 6px;
    outline: none;
    font-family: inherit;
    color-scheme: dark;
  }
  .field-input:focus {
    border-color: var(--accent-color, #7aa2f7);
  }
</style>
