<script lang="ts">
  import type { TreeNode } from '../blockTree.svelte';
  import { resolveFileUrl } from '../blobUrl';

  let { node, isEditing }: { node: TreeNode; isEditing: boolean } = $props();

  let fileHash = $derived(node.properties?.file_hash as string | undefined);
  let filename = $derived((node.properties?.filename as string) ?? node.name);
  let mime = $derived((node.properties?.mime as string) ?? 'application/octet-stream');
  let size = $derived(node.properties?.size as number | undefined);
  let missing = $derived(!fileHash);

  let fileUrl = $state<string | null>(null);

  $effect(() => {
    const hash = fileHash;
    const m = mime;
    if (!hash) {
      fileUrl = null;
      return;
    }
    resolveFileUrl(hash, m).then((url) => {
      fileUrl = url || null;
    });
  });

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function extensionIcon(name: string): string {
    const ext = name.split('.').pop()?.toLowerCase() ?? '';
    const icons: Record<string, string> = {
      doc: 'DOC', docx: 'DOC', odt: 'DOC',
      xls: 'XLS', xlsx: 'XLS', ods: 'XLS',
      ppt: 'PPT', pptx: 'PPT', odp: 'PPT',
      txt: 'TXT', md: 'MD', csv: 'CSV',
      zip: 'ZIP', gz: 'GZ', tar: 'TAR',
      mp3: 'MP3', wav: 'WAV', ogg: 'OGG',
      mp4: 'MP4', mkv: 'MKV', avi: 'AVI',
    };
    return icons[ext] ?? 'FILE';
  }
</script>

{#if isEditing}
  <div class="file-edit">
    <div class="file-details">
      <span class="file-badge">{extensionIcon(filename)}</span>
      <div class="file-info">
        <span class="file-name">{filename}</span>
        <span class="file-mime">{mime}</span>
        {#if size}
          <span class="file-size">{formatSize(size)}</span>
        {/if}
      </div>
    </div>
    {#if fileUrl}
      <a href={fileUrl} download={filename} class="file-download" aria-label="Download {filename}">Download</a>
    {/if}
  </div>
{:else}
  <span class="file-nav">
    {#if missing}
      <span class="file-missing">Missing: {filename}</span>
    {:else}
      <span class="file-badge-sm">{extensionIcon(filename)}</span>
      <span class="file-name-nav">{filename}</span>
      {#if size}
        <span class="file-size-nav">{formatSize(size)}</span>
      {/if}
    {/if}
  </span>
{/if}

<style>
  .file-nav {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }

  .file-badge-sm {
    background: var(--bg-tertiary, #24283b);
    color: var(--text-muted, #7a85b8);
    font-size: 8px;
    font-weight: 700;
    padding: 1px 3px;
    border-radius: 2px;
    letter-spacing: 0.5px;
  }

  .file-name-nav {
    color: var(--text-primary, #ccc);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-size-nav {
    color: var(--text-muted, #7a85b8);
    font-size: 11px;
    flex-shrink: 0;
  }

  .file-edit {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .file-details {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .file-badge {
    background: var(--bg-tertiary, #24283b);
    color: var(--text-muted, #7a85b8);
    font-size: 10px;
    font-weight: 700;
    padding: 4px 6px;
    border-radius: 3px;
    letter-spacing: 0.5px;
    flex-shrink: 0;
  }

  .file-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .file-name {
    color: var(--text-primary, #ccc);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-mime {
    color: var(--text-muted, #7a85b8);
    font-size: 10px;
  }

  .file-size {
    color: var(--text-muted, #7a85b8);
    font-size: 10px;
  }

  .file-download {
    align-self: flex-start;
    color: var(--link-color, #7aa2f7);
    text-decoration: none;
    font-size: 11px;
    cursor: pointer;
  }

  .file-download:hover {
    text-decoration: underline;
  }

  .file-missing {
    color: var(--text-muted, #7a85b8);
    font-style: italic;
    font-size: 12px;
  }
</style>
