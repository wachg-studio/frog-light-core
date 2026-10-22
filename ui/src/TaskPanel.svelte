<script>
  import { onMount } from 'svelte';
  import { t } from './i18n.svelte.js';

  // 任务详情：正在做什么、进度、正在下载哪几个文件、接下来排着什么、最近完成了什么。
  let {
    current, // { id, label, verb } | null
    launching = null, // 准备游戏中（不在队列里）：{ label } | null
    queue, // [{ id, label }]
    history, // [{ id, label, status: 'ok' | 'err' | 'cancelled', text }]
    ring,
    stage,
    detail,
    files,
    cancelling,
    onCancel, // 取消正在进行的
    onDrop, // (id) 取消排队中的某一个
    onCancelAll,
    onClose,
  } = $props();

  const active = $derived(current ?? launching);
  const pct = $derived(Math.round(ring * 100));

  onMount(() => {
    const key = (e) => { if (e.key === 'Escape') onClose(); };
    const away = (e) => { if (!e.target.closest?.('.tpanel, .task')) onClose(); };
    addEventListener('keydown', key);
    addEventListener('pointerdown', away, true);
    return () => {
      removeEventListener('keydown', key);
      removeEventListener('pointerdown', away, true);
    };
  });
</script>

<div class="tpanel glass" role="dialog" aria-label={t('下载任务')}>
  <header>
    <h4>{t('下载任务')}</h4>
    {#if active && (queue.length || cancelling)}
      <button class="link" onclick={onCancelAll}>{t('全部取消')}</button>
    {/if}
  </header>

  {#if active}
    <section class="now">
      <div class="row1">
        <b class="nm">{active.label}</b>
        <span class="pct">{pct}%</span>
      </div>
      <div class="bar" role="progressbar" aria-valuenow={pct} aria-valuemin="0" aria-valuemax="100"><i style:width="{pct}%"></i></div>
      <div class="row2">
        <span>{cancelling ? t('正在取消…') : t(stage || '准备中')}{detail ? ` · ${detail}` : ''}</span>
        <button class="mini danger" onclick={onCancel} disabled={cancelling}>{t('取消')}</button>
      </div>
      {#if files.length}
        <ul class="files" aria-label={t('正在下载的文件')}>
          {#each files as f (f)}<li><span class="dot"></span>{f}</li>{/each}
        </ul>
      {/if}
    </section>
  {:else}
    <p class="idle">{t('现在没有进行中的任务')}</p>
  {/if}

  {#if queue.length}
    <section>
      <h5>{t('接下来 · {n} 个', { n: queue.length })}</h5>
      <ol class="queue">
        {#each queue as q, i (q.id)}
          <li>
            <span class="idx">{i + 1}</span>
            <span class="qn">{q.label}</span>
            <button class="x" onclick={() => onDrop(q.id)} aria-label={t('从队列移除')} title={t('从队列移除')}>×</button>
          </li>
        {/each}
      </ol>
    </section>
  {/if}

  {#if history.length}
    <section>
      <h5>{t('最近')}</h5>
      <ul class="hist">
        {#each history as h (h.id)}
          <li class={h.status}>
            <span class="mark" aria-hidden="true">{h.status === 'ok' ? '✓' : h.status === 'cancelled' ? '–' : '!'}</span>
            <span class="hn">{h.label}</span>
            <small>{h.status === 'ok' ? t('完成') : h.status === 'cancelled' ? t('已取消') : (h.text || t('失败'))}</small>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

<style>
  .tpanel { position: fixed; top: 44px; right: 14px; z-index: 25; width: 340px; max-height: min(480px, calc(100vh - 64px)); overflow: auto; padding: 14px 16px 16px; background: var(--pop); display: flex; flex-direction: column; gap: 12px; animation: pop 0.2s var(--ease); }
  @keyframes pop { from { opacity: 0; transform: translateY(-6px); } }
  header { display: flex; align-items: center; justify-content: space-between; }
  h4 { font-size: 14px; font-weight: 600; }
  h5 { font-size: 12px; font-weight: 600; color: var(--text-dim); margin-bottom: 6px; }
  .link { font-size: 12px; color: var(--blue); }
  .link:hover { text-decoration: underline; }
  .idle { font-size: 13px; color: var(--text-dim); }

  .now { display: flex; flex-direction: column; gap: 8px; padding: 12px; border-radius: 14px; background: var(--fill); }
  .row1 { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; }
  .nm { font-size: 14px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pct { font-size: 13px; font-variant-numeric: tabular-nums; color: var(--text-dim); }
  .bar { height: 5px; border-radius: 3px; background: var(--ring-track); overflow: hidden; }
  .bar i { display: block; height: 100%; border-radius: 3px; background: linear-gradient(90deg, var(--text) 0%, color-mix(in srgb, var(--text) 55%, transparent) 50%, var(--text) 100%); background-size: 200% 100%; animation: sheen 1.6s linear infinite; transition: width 0.3s var(--ease); }
  @keyframes sheen { to { background-position: -200% 0; } }
  .row2 { display: flex; align-items: center; justify-content: space-between; gap: 10px; font-size: 12px; color: var(--text-dim); }
  .row2 span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .mini.danger { color: var(--danger); flex: none; }
  .files { list-style: none; padding: 0; display: flex; flex-direction: column; gap: 3px; font-size: 12px; color: var(--text-dim); }
  .files li { display: flex; align-items: center; gap: 8px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { width: 5px; height: 5px; border-radius: 50%; background: var(--text-dim); flex: none; animation: blink 1.2s ease-in-out infinite; }
  @keyframes blink { 50% { opacity: 0.3; } }

  .queue, .hist { list-style: none; padding: 0; display: flex; flex-direction: column; }
  .queue li, .hist li { display: flex; align-items: center; gap: 10px; min-height: 32px; font-size: 13px; }
  .idx { width: 18px; height: 18px; border-radius: 9px; background: var(--fill); font-size: 11px; display: grid; place-items: center; color: var(--text-dim); flex: none; }
  .qn, .hn { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x { width: 22px; height: 22px; border-radius: 11px; color: var(--text-dim); font-size: 16px; line-height: 1; flex: none; }
  .x:hover { background: var(--fill-hover); color: var(--danger); }
  .mark { width: 18px; text-align: center; flex: none; }
  .hist .ok .mark { color: #1f8a3c; }
  .hist .err .mark { color: var(--danger); }
  .hist small { max-width: 45%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-dim); font-size: 12px; }
  .hist .err small { color: var(--danger); }
</style>
