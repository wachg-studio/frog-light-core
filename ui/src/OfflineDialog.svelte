<script>
  import { onMount } from 'svelte';
  import { settings } from './settings.svelte.js';
  import { t } from './i18n.svelte.js';

  let { onClose, onDone } = $props();

  // 第一题两种回答都算通过；其余每题只有一个正确做法。
  const questions = [
    { q: '你目前的情况是？', options: ['我已经购买了 Minecraft', '我暂时没有购买，想先免费试玩一下'], correct: null },
    { q: '离线模式可以用来做什么？', options: ['没有网络或不想登录时，用本地玩家名玩自己的存档', '进入正版服务器，或者使用正版账号的皮肤'], correct: 0 },
    { q: '下面哪种做法是不允许的？', options: ['只在单机存档里使用离线模式', '把游戏文件发给没买游戏的人，或在盗版服务器上玩'], correct: 1 },
    { q: '如果你喜欢 Minecraft，最好的支持方式是？', options: ['购买正版，支持开发者和持续更新', '一直用离线模式就够了'], correct: 0 },
  ];

  let answers = $state(questions.map(() => -1));
  let agree = $state(false);
  const allAnswered = $derived(answers.every((a) => a >= 0));
  const allRight = $derived(questions.every((q, i) => q.correct === null || answers[i] === q.correct));
  const canFinish = $derived(allAnswered && allRight && agree);
  const isWrong = (i) => answers[i] >= 0 && questions[i].correct !== null && answers[i] !== questions[i].correct;

  function finish() {
    if (!canFinish) return;
    settings.offlineEnabled = true;
    settings.accountType = 'offline';
    onDone();
  }

  onMount(() => {
    const key = (e) => { if (e.key === 'Escape') onClose(); };
    addEventListener('keydown', key);
    return () => removeEventListener('keydown', key);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
  <div class="dlg glass" role="dialog" aria-modal="true" aria-label={t('离线模式')}>
    <header>
      <h2>{t('离线模式')}</h2>
      <p class="intro">{t('登录正版账号后会自动解锁离线模式，不需要答题')}</p>
    </header>

    <div class="body">
      {#each questions as q, i}
        <fieldset>
          <legend><span class="n">{i + 1}</span>{t(q.q)}</legend>
          {#each q.options as o, j}
            <label class="opt" class:on={answers[i] === j} class:bad={isWrong(i) && answers[i] === j}>
              <input type="radio" name={`q${i}`} checked={answers[i] === j} onchange={() => (answers[i] = j)} />
              <span>{t(o)}</span>
            </label>
          {/each}
          {#if isWrong(i)}<p class="hint">{t('答错了，再想想')}</p>{/if}
        </fieldset>
      {/each}

      <label class="agree">
        <input type="checkbox" bind:checked={agree} />
        <span>{t('我同意遵守 Minecraft 最终用户许可协议，不把离线模式用于盗版、商业用途或传播游戏文件。')}</span>
      </label>
    </div>

    <footer>
      <button class="ghost" onclick={onClose}>{t('关闭')}</button>
      <button class="primary" disabled={!canFinish} onclick={finish}>{t('完成')}</button>
    </footer>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 32; display: grid; place-items: center; padding: 32px 20px; background: rgba(0, 0, 0, 0.3); backdrop-filter: blur(5px); animation: fade 0.2s var(--ease); }
  @keyframes fade { from { opacity: 0; } }

  .dlg { width: min(560px, 100%); max-height: min(760px, 90vh); display: flex; flex-direction: column; background: var(--pop); border-radius: 20px; animation: rise 0.3s var(--ease); }
  @keyframes rise { from { opacity: 0; transform: translateY(10px) scale(0.985); } }

  header { padding: 22px 26px 12px; display: flex; flex-direction: column; gap: 5px; }
  h2 { font-size: 20px; font-weight: 600; letter-spacing: -0.02em; }
  .intro { font-size: 12.5px; line-height: 1.6; color: var(--text-dim); }

  .body { padding: 4px 26px 18px; overflow: auto; }
  fieldset { border: 0; padding: 0; margin: 0 0 18px; }
  legend { display: flex; align-items: baseline; gap: 8px; font-size: 14px; font-weight: 500; line-height: 1.5; margin-bottom: 8px; }
  .n { display: inline-grid; place-items: center; width: 18px; height: 18px; border-radius: 6px; background: var(--fill); font-size: 11px; font-weight: 600; color: var(--text-dim); flex: none; }

  .opt { display: flex; align-items: flex-start; gap: 9px; padding: 8px 10px; border-radius: 10px; font-size: 13px; line-height: 1.55; cursor: pointer; transition: background 0.15s var(--ease), color 0.15s var(--ease); }
  .opt:hover { background: var(--fill); }
  .opt.on { background: var(--fill-hover); }
  .opt input { margin-top: 3px; accent-color: var(--blue); flex: none; }
  .opt.bad { color: var(--danger); }

  .hint { margin: -2px 0 0 28px; font-size: 12px; color: var(--danger); }

  .agree { display: flex; align-items: flex-start; gap: 9px; padding: 12px 10px 0 10px; border-top: 1px solid var(--hairline); font-size: 12.5px; line-height: 1.6; color: var(--text-dim); cursor: pointer; }
  .agree input { margin-top: 3px; accent-color: var(--blue); flex: none; }

  footer { display: flex; justify-content: flex-end; gap: 10px; padding: 14px 26px 20px; border-top: 1px solid var(--hairline); }
  button { height: 34px; padding: 0 20px; border-radius: 17px; font-size: 13.5px; font-weight: 500; transition: transform 0.2s var(--ease), background 0.2s var(--ease), opacity 0.2s; }
  .primary { background: var(--btn); color: var(--btn-text); font-weight: 600; }
  .primary:hover:not(:disabled) { transform: scale(1.04); }
  .primary:disabled { opacity: 0.35; cursor: not-allowed; }
  .ghost { color: var(--text-dim); }
  .ghost:hover { background: var(--fill); color: var(--text); }
</style>
