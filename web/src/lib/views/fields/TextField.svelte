<script lang="ts">
  import type { SchemaField } from '../../types'

  let { value, field, onChange }: { value: unknown; field: SchemaField; onChange: (v: string) => void } = $props()
</script>

<textarea
  class="field-textarea"
  value={value as string ?? ''}
  placeholder={field.name}
  aria-label={field.name}
  rows="3"
  oninput={(e) => onChange((e.target as HTMLTextAreaElement).value)}
></textarea>

<style>
  .field-textarea {
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-secondary, #1a1b26);
    border: 1px solid var(--border-color, #333);
    border-radius: 3px;
    color: var(--text-primary, #ccc);
    font-size: 12px;
    padding: 4px 6px;
    outline: none;
    font-family: inherit;
    resize: vertical;
    min-height: 48px;
  }
  .field-textarea:focus {
    border-color: var(--accent-color, #7aa2f7);
  }
</style>
