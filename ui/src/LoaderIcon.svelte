<script>
  // 模组加载器图标。这是 FLC 自己绘制的简化图形（用各加载器的惯用意象和配色），不是官方商标图。
  // kind: fabric | legacyfabric | quilt | forge | neoforge | optifine | liteloader
  let { kind, size = 28 } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 32 32" role="img" aria-label={kind} class="licon">
  {#if kind === 'fabric'}
    <rect width="32" height="32" rx="8" fill="#e3d9bf" />
    <path d="M8 7h13l3 3v15H8z" fill="#b9a77a" />
    <path d="M8 7h13l3 3h-5a1 1 0 0 1-1-1V7z" fill="#8f7d52" opacity=".55" />
    <path d="M12 14h8M12 18h8M12 22h5" stroke="#f6f0dd" stroke-width="1.6" stroke-linecap="round" stroke-dasharray="2.4 2.4" />
  {:else if kind === 'legacyfabric'}
    <rect width="32" height="32" rx="8" fill="#e3d9bf" />
    <path d="M8 7h13l3 3v15H8z" fill="#a89468" />
    <path d="M8 7h13l3 3h-5a1 1 0 0 1-1-1V7z" fill="#7c6a40" opacity=".55" />
    <path d="M12 14h8M12 18h8M12 22h5" stroke="#f6f0dd" stroke-width="1.6" stroke-linecap="round" stroke-dasharray="2.4 2.4" />
    <circle cx="24" cy="24" r="6" fill="#5b4a28" /><path d="M24 20.5v4.2h2.6" stroke="#f6f0dd" stroke-width="1.5" stroke-linecap="round" fill="none" />
  {:else if kind === 'quilt'}
    <rect width="32" height="32" rx="8" fill="#2c2250" />
    <rect x="6" y="6" width="9.5" height="9.5" rx="2" fill="#8e6bf0" />
    <rect x="16.5" y="6" width="9.5" height="9.5" rx="2" fill="#4fa3f7" />
    <rect x="6" y="16.5" width="9.5" height="9.5" rx="2" fill="#5fd3b0" />
    <rect x="16.5" y="16.5" width="9.5" height="9.5" rx="2" fill="#c9a6ff" />
    <path d="M8 10.7h5.5M18.5 10.7H24M8 21.3h5.5M18.5 21.3H24" stroke="#fff" stroke-opacity=".7" stroke-width="1.2" stroke-linecap="round" stroke-dasharray="1.6 1.8" />
  {:else if kind === 'forge'}
    <rect width="32" height="32" rx="8" fill="#243047" />
    <path d="M6 10h20v3.2h-5c.2 3.4 2.2 4.2 4 4.8v3.2H9v-3.2c2.2-.6 4.4-1.4 4.6-4.8H6z" fill="#e8edf5" />
    <path d="M9 24.5h14" stroke="#ff8a3d" stroke-width="2.2" stroke-linecap="round" />
  {:else if kind === 'neoforge'}
    <rect width="32" height="32" rx="8" fill="#e8741f" />
    <path d="M6 11h20v3.2h-5c.2 3.4 2.2 4.2 4 4.8v3H9v-3c2.2-.6 4.4-1.4 4.6-4.8H6z" fill="#fff" />
    <path d="M22.5 5.5l1.1 2.4 2.4 1.1-2.4 1.1-1.1 2.4-1.1-2.4-2.4-1.1 2.4-1.1z" fill="#fff" opacity=".9" />
  {:else if kind === 'optifine'}
    <rect width="32" height="32" rx="8" fill="#c63d3d" />
    <path d="M16 6l2.6 7.4L26 16l-7.4 2.6L16 26l-2.6-7.4L6 16l7.4-2.6z" fill="#fff" />
  {:else if kind === 'liteloader'}
    <rect width="32" height="32" rx="8" fill="#2f86d6" />
    <path d="M24 7c-8 0-14 5-15 13l-1.5 5 1.4.6L11 22.5c7-.3 11.5-5 13-15.5z" fill="#fff" />
    <path d="M10 22c3-4.5 6.5-7.5 11-9.5" stroke="#2f86d6" stroke-width="1.2" stroke-linecap="round" />
  {:else}
    <rect width="32" height="32" rx="8" fill="#9aa0aa" />
    <path d="M16 8l8 4.6v9L16 26l-8-4.4v-9z" fill="#fff" opacity=".85" />
  {/if}
</svg>

<style>
  .licon { flex: none; filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.18)); }
</style>
