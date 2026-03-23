<script lang="ts">
  import type { TreeNode } from '../blockTree.svelte';
  import { resolveFileUrl } from '../blobUrl';

  let { node, isEditing }: { node: TreeNode; isEditing: boolean } = $props();

  let fileHash = $derived(node.properties?.file_hash as string | undefined);
  let filename = $derived((node.properties?.filename as string) ?? node.name);
  let size = $derived(node.properties?.size as number | undefined);
  let mime = $derived((node.properties?.mime as string) ?? 'application/pdf');
  let missing = $derived(!fileHash);

  let pdfUrl = $state<string | null>(null);

  $effect(() => {
    const hash = fileHash;
    const m = mime;
    if (!hash) {
      pdfUrl = null;
      return;
    }
    resolveFileUrl(hash, m).then((url) => {
      pdfUrl = url || null;
    });
  });

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

{#if isEditing}
  <div class="pdf-edit">
    {#if pdfUrl}
      <!-- Use <object> instead of <iframe> — more reliable across browsers for PDF -->
      <object data={pdfUrl} type="application/pdf" class="pdf-viewer" aria-label="PDF preview: {filename}">
        <div class="pdf-fallback">
          <span class="pdf-icon-lg">PDF</span>
          <span>Preview not available.</span>
          <a href={pdfUrl} download={filename} class="pdf-download-link">Download PDF</a>
        </div>
      </object>
    {:else if missing}
      <div class="pdf-missing">PDF file not available</div>
    {:else}
      <div class="pdf-loading">Loading...</div>
    {/if}
    <div class="pdf-meta">
      <span class="meta-label">{filename}</span>
      {#if size}
        <span class="meta-size">{formatSize(size)}</span>
      {/if}
      {#if pdfUrl}
        <a href={pdfUrl} download={filename} class="meta-download" aria-label="Download {filename}">Download</a>
      {/if}
    </div>
  </div>
{:else}
  <span class="pdf-nav">
    {#if missing}
      <span class="pdf-missing-inline">Missing: {filename}</span>
    {:else}
      <span class="pdf-icon">PDF</span>
      <span class="pdf-name">{filename}</span>
      {#if size}
        <span class="pdf-size">{formatSize(size)}</span>
      {/if}
    {/if}
  </span>
{/if}

<style>
  .pdf-nav {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }

  .pdf-icon {
    background: #c53030;
    color: white;
    font-size: 9px;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 2px;
    letter-spacing: 0.5px;
  }

  .pdf-icon-lg {
    background: #c53030;
    color: white;
    font-size: 14px;
    font-weight: 700;
    padding: 4px 8px;
    border-radius: 3px;
    letter-spacing: 0.5px;
  }

  .pdf-name {
    color: var(--text-primary, #ccc);
  }

  .pdf-size {
    color: var(--text-muted, #7a85b8);
    font-size: 11px;
  }

  .pdf-edit {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .pdf-viewer {
    width: 100%;
    height: 400px;
    border: 1px solid var(--border-color, #333);
    border-radius: 4px;
    background: var(--bg-secondary, #1a1b26);
  }

  .pdf-fallback {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 100%;
    padding: 20px;
    color: var(--text-muted, #7a85b8);
    font-size: 12px;
  }

  .pdf-download-link {
    color: var(--link-color, #7aa2f7);
    text-decoration: none;
  }

  .pdf-download-link:hover {
    text-decoration: underline;
  }

  .pdf-meta {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 11px;
    color: var(--text-muted, #7a85b8);
  }

  .meta-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta-size {
    flex-shrink: 0;
    opacity: 0.7;
  }

  .meta-download {
    flex-shrink: 0;
    color: var(--link-color, #7aa2f7);
    text-decoration: none;
    cursor: pointer;
  }

  .meta-download:hover {
    text-decoration: underline;
  }

  .pdf-missing, .pdf-missing-inline {
    color: var(--text-muted, #7a85b8);
    font-style: italic;
    font-size: 12px;
  }

  .pdf-missing {
    padding: 12px;
    border: 1px dashed var(--border-color, #333);
    border-radius: 4px;
    text-align: center;
  }

  .pdf-loading {
    color: var(--text-muted, #7a85b8);
    font-size: 11px;
  }
</style>
