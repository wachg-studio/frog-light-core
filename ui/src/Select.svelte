<script>
  // 自绘下拉框：不使用系统原生的 <select> 弹层，样式、深浅色和键盘操作都由软件自己控制。
  let { value = $bindable(''), options = [], label = '', width = 'auto' } = $props();

  // 毛玻璃面板自带 backdrop-filter，会把里面 position:fixed 的元素变成相对面板定位，
  // 所以弹层要挂到 <body> 下，才能按屏幕坐标摆放。
  function portal(node) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  let open = $state(false);
  let btn;
  let pos = $state({ top: 0, left: 0, minWidth: 0, up: false });
  let active = $state(0);

  const current = $derived(options.find(([v]) => v === value)?.[1] ?? '');

  function toggle() {
    if (open) return close();
    const r = btn.getBoundingClientRect();
    const h = Math.min(options.length * 34 + 12, 280);
    const up = r.bottom + h + 12 > innerHeight && r.top > h; // 下面放不下就向上展开
    pos = { top: up ? r.top - 6 : r.bottom + 6, left: r.right, minWidth: r.width, up };
    active = Math.max(0, options.findIndex(([v]) => v === value));
    open = true;
  }

  function close() { open = false; }

  function pick(v) {
    value = v;
    close();
    btn.focus();
  }

  function onKey(e) {
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); toggle(); }
      return;
    }
    if (e.key === 'Escape') { e.preventDefault(); close(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); active = Math.min(options.length - 1, active + 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); active = Math.max(0, active - 1); }
    else if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); pick(options[active][0]); }
  }

  $effect(() => {
    if (!open) return;
    const off = (e) => { if (!e.target.closest?.('.sel-pop') && e.target !== btn && !btn.contains(e.target)) close(); };
    addEventListener('pointerdown', off, true);
    // 滚轮在弹层里是用来滚动选项的，不能关掉它；在弹层外面滚动页面才关闭
    const onWheel = (e) => { if (!e.target.closest?.('.sel-pop')) close(); };
    addEventListener('resize', close);
    addEventListener('wheel', onWheel, { passive: true });
    return () => {
      removeEventListener('pointerdown', off, true);
      removeEventListener('resize', close);
      removeEventListener('wheel', onWheel);
    };
  });
</script>

<button class="sel-btn" bind:this={btn} onclick={toggle} onkeydown={onKey} aria-haspopup="listbox" aria-expanded={open} aria-label={label} style:width={width}>
  <span>{current}</span>
  <svg width="10" height="6" viewBox="0 0 10 6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M1 1l4 4 4-4" /></svg>
</button>

{#if open}
  <div class="sel-pop" use:portal class:up={pos.up} role="listbox" aria-label={label} style="top:{pos.top}px; left:{pos.left}px; min-width:{pos.minWidth}px">
    {#each options as [v, text], i (v)}
      <button class="sel-opt" class:on={v === value} class:active={i === active} role="option" aria-selected={v === value} onpointermove={() => (active = i)} onclick={() => pick(v)}>
        <span>{text}</span>
        {#if v === value}<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .sel-btn {
    display: inline-flex; align-items: center; justify-content: space-between; gap: 10px; height: 30px; padding: 0 10px 0 12px;
    border-radius: 8px; background: var(--glass-solid); font-size: 13px; box-shadow: inset 0 0 0 1px var(--hairline);
    transition: background 0.2s; white-space: nowrap;
  }
  .sel-btn:hover { background: var(--fill-hover); }
  .sel-btn svg { opacity: 0.5; flex: none; }
  .sel-btn[aria-expanded='true'] { box-shadow: inset 0 0 0 1px var(--blue); }

  .sel-pop {
    position: fixed; z-index: 40; transform: translateX(-100%); max-height: 280px; overflow: auto; padding: 5px;
    border-radius: 12px; background: var(--pop); box-shadow: 0 0 0 1px var(--hairline), 0 12px 36px rgba(0, 0, 0, 0.22);
    backdrop-filter: blur(30px) saturate(1.8); -webkit-backdrop-filter: blur(30px) saturate(1.8);
    animation: pop 0.16s var(--ease);
  }
  .sel-pop.up { transform: translate(-100%, -100%); }
  @keyframes pop { from { opacity: 0; margin-top: -4px; } }
  .sel-opt { display: flex; align-items: center; justify-content: space-between; gap: 14px; width: 100%; height: 32px; padding: 0 10px; border-radius: 8px; font-size: 13px; text-align: left; white-space: nowrap; }
  .sel-opt.active { background: var(--blue); color: #fff; }
  .sel-opt svg { flex: none; }
</style>
