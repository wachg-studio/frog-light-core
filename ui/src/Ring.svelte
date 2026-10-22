<script>
  let { value = 0, size = 220, stroke = 5, children } = $props();
  const r = $derived((size - stroke) / 2);
  const c = $derived(2 * Math.PI * r);
</script>

<div class="ring" style="width:{size}px;height:{size}px">
  <svg width={size} height={size} viewBox="0 0 {size} {size}">
    <circle class="track" cx={size / 2} cy={size / 2} {r} stroke-width={stroke} fill="none" />
    <circle
      class="bar"
      cx={size / 2}
      cy={size / 2}
      {r}
      stroke-width={stroke}
      fill="none"
      stroke-linecap="round"
      stroke-dasharray={c}
      stroke-dashoffset={c * (1 - Math.min(1, Math.max(0, value)))}
      transform="rotate(-90 {size / 2} {size / 2})"
    />
  </svg>
  <div class="inner">{@render children?.()}</div>
</div>

<style>
  .ring { position: relative; }
  svg { position: absolute; inset: 0; overflow: visible; }
  .track { stroke: var(--ring-track); }
  .bar { stroke: var(--text); transition: stroke-dashoffset 0.4s var(--ease); }
  .inner { position: absolute; inset: 0; display: grid; place-items: center; text-align: center; }
</style>
