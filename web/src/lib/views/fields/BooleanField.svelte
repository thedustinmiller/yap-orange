<script lang="ts">
  import type { SchemaField } from '../../types'

  let { value, field, onChange }: { value: unknown; field: SchemaField; onChange: (v: boolean) => void } = $props()
</script>

<label class="toggle">
  <input
    type="checkbox"
    checked={value as boolean ?? false}
    aria-label={field.name}
    onchange={(e) => onChange((e.target as HTMLInputElement).checked)}
  />
  <span class="toggle-track"></span>
</label>

<style>
  .toggle {
    display: inline-flex;
    align-items: center;
    cursor: pointer;
  }
  .toggle input[type="checkbox"] {
    opacity: 0;
    width: 0;
    height: 0;
    position: absolute;
  }
  .toggle-track {
    display: inline-block;
    width: 32px;
    height: 16px;
    background: var(--bg-tertiary, #2d2d3a);
    border-radius: 8px;
    border: 1px solid var(--border-color, #333);
    position: relative;
    transition: background 0.15s;
  }
  .toggle-track::after {
    content: '';
    position: absolute;
    left: 2px;
    top: 50%;
    transform: translateY(-50%);
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--text-muted, #555);
    transition: left 0.15s, background 0.15s;
  }
  .toggle input:checked + .toggle-track {
    background: var(--accent-color, #7aa2f7);
    border-color: var(--accent-color, #7aa2f7);
  }
  .toggle input:checked + .toggle-track::after {
    left: 18px;
    background: white;
  }
</style>
