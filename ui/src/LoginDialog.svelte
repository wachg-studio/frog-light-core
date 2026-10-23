<script>
  import { onMount, onDestroy } from 'svelte';
  import { call, inTauri, CANCELLED } from './api.js';
  import { t } from './i18n.svelte.js';
  import { REPO_URL } from './about.js';

  // 微软账号登录（设备码流程）：显示一个登录码，用户在浏览器里打开微软的页面输入，这里等它完成。
  let { settings, onDone, onClose, onSettings } = $props();

  let status = $state('starting'); // starting | waiting | error
  let device = $state(null);
  let error = $state('');
  let remain = $state(0);
  let copied = $state(false);
  let timer;
  let closed = false;

  const minutes = $derived(`${Math.floor(remain / 60)}:${String(remain % 60).padStart(2, '0')}`);
  const needsClientId = $derived(error.includes('Client ID'));

  async function begin() {
    status = 'starting';
    error = '';
    try {
      device = await call('ms_login_start', { settings: settings() });
    } catch (e) {
      status = 'error';
      error = String(e);
      return;
    }
    status = 'waiting';
    remain = device.expiresIn;
    clearInterval(timer);
    timer = setInterval(() => { if (remain > 0) remain -= 1; }, 1000);
    openPage();
    try {
      const info = await call('ms_login_finish', { device, settings: settings() });
      clearInterval(timer);
      if (!closed) onDone(info);
    } catch (e) {
      clearInterval(timer);
      if (closed || String(e) === CANCELLED) return;
      status = 'error';
      error = String(e);
    }
  }

  function openPage() {
    if (!device) return;
    if (inTauri) call('open_url', { url: device.verificationUri }).catch(() => {});
    else window.open(device.verificationUri, '_blank', 'noopener');
  }

  function openClientIdDoc() {
    if (!REPO_URL) return;
    const url = `${REPO_URL}/blob/main/docs/client-id.md`;
    if (inTauri) call('open_url', { url }).catch(() => {});
    else window.open(url, '_blank', 'noopener');
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(device.userCode);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // 剪贴板不可用就算了，用户可以手动抄
    }
  }

  async function cancel() {
    closed = true;
    if (status === 'waiting') await call('cancel_task').catch(() => {});
    onClose();
  }

  onMount(() => {
    begin();
    const key = (e) => { if (e.key === 'Escape') cancel(); };
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  });
  onDestroy(() => clearInterval(timer));
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => { if (e.target === e.currentTarget) cancel(); }}>
  <div class="dlg glass" role="dialog" aria-modal="true" aria-label={t('登录微软账号')}>
    <h2>{t('登录微软账号')}</h2>

    {#if status === 'starting'}
      <p class="dim center"><span class="spin" aria-hidden="true"></span> {t('正在连接微软…')}</p>
    {:else if status === 'waiting'}
      <ol class="steps">
        <li>{t('在打开的网页里输入下面的登录码')}</li>
        <li>{t('用你的微软账号登录并确认')}</li>
        <li>{t('回到这里，登录会自动完成')}</li>
      </ol>
      <button class="code" onclick={copy} title={t('点击复制')} aria-label={t('复制登录码')}>
        <b>{device.userCode}</b>
        <small>{copied ? t('已复制') : t('点击复制')}</small>
      </button>
      <div class="actions">
        <button class="primary" onclick={openPage}>{t('打开登录网页')}</button>
      </div>
      <p class="dim center small"><span class="spin" aria-hidden="true"></span> {t('等待你完成登录… 剩余 {time}', { time: minutes })}</p>
      <p class="dim center small url">{device.verificationUri}</p>
    {:else}
      <p class="err">{error}</p>
      {#if needsClientId}
        <p class="dim small">{t('正版登录需要一个在 Azure 注册并通过 Minecraft 审核的应用 Client ID。拿到后填到 设置 → 账号 里即可。')}</p>
      {/if}
      <div class="actions">
        {#if needsClientId}
          <button class="primary" onclick={() => { onClose(); onSettings(); }}>{t('去设置')}</button>
          {#if REPO_URL}<button class="ghost" onclick={openClientIdDoc}>{t('获取方法')}</button>{/if}
        {:else}
          <button class="primary" onclick={begin}>{t('重试')}</button>
        {/if}
      </div>
    {/if}

    <button class="close" onclick={cancel}>{status === 'waiting' ? t('取消登录') : t('关闭')}</button>
  </div>
</div>

<style>
  .scrim { backdrop-filter: blur(5px); position: fixed; inset: 0; z-index: 30; display: grid; place-items: center; padding: 40px 24px; background: rgba(0, 0, 0, 0.28); animation: fade 0.2s var(--ease); }
  @keyframes fade { from { opacity: 0; } }
  .dlg { width: min(420px, 100%); display: flex; flex-direction: column; gap: 14px; padding: 24px 26px 18px; background: var(--pop); animation: rise 0.35s var(--ease); }
  @keyframes rise { from { opacity: 0; transform: translateY(10px) scale(0.98); } }
  h2 { font-size: 20px; font-weight: 600; letter-spacing: -0.02em; }
  .steps { padding-left: 20px; display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--text-dim); }
  .code { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 14px; border-radius: 14px; background: var(--fill); transition: background 0.2s; }
  .code:hover { background: var(--fill-hover); }
  .code b { font: 600 30px/1.2 'Cascadia Mono', Consolas, monospace; letter-spacing: 0.14em; user-select: text; }
  .code small { font-size: 11px; color: var(--text-dim); }
  .actions { display: flex; justify-content: center; }
  .primary { height: 36px; padding: 0 24px; border-radius: 18px; background: var(--btn); color: var(--btn-text); font-size: 14px; font-weight: 600; transition: transform 0.2s var(--ease); }
  .primary:hover { transform: scale(1.04); }
  .ghost { height: 36px; padding: 0 18px; border-radius: 18px; color: var(--text-dim); font-size: 13.5px; }
  .ghost:hover { background: var(--fill); color: var(--text); }
  .dim { color: var(--text-dim); }
  .small { font-size: 12px; line-height: 1.5; }
  .center { display: flex; align-items: center; justify-content: center; gap: 8px; text-align: center; }
  .url { user-select: text; word-break: break-all; }
  .err { font-size: 13px; line-height: 1.5; color: var(--danger); user-select: text; }
  .close { align-self: center; font-size: 13px; color: var(--text-dim); padding: 4px 12px; border-radius: 8px; }
  .close:hover { background: var(--fill); color: var(--text); }
</style>
