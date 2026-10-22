<script>
  import { settings, GAME_LANGS, checkOfflineKey } from './settings.svelte.js';
  import { t, LANGS } from './i18n.svelte.js';
  import { pickFolder, inTauri, call } from './api.js';
  import Select from './Select.svelte';
  import Head from './Head.svelte';
  import { activeSkin } from './skins.svelte.js';
  import { VERSION, DEVELOPER, LICENSE, REPO_URL } from './about.js';

  let { panel = $bindable(null), defaultRoot = '', defaultDataDir = '', accounts = [], onSkin = () => {}, onLogin = () => {}, onRemoveAccount = () => {} } = $props();

  // 离线模式藏得比较深：开发者选项 → 测试功能 → 其他 → 连点版本号 3 次，才出现密钥输入框。
  let devOpen = $state(false);
  let taps = $state(0);
  let keyRow = $state(false);
  let keyInput = $state('');
  let keyErr = $state(false);

  function tapVersion() {
    if (keyRow || settings.offlineEnabled) return;
    if (++taps >= 3) keyRow = true;
  }

  async function unlockOffline() {
    if (!(await checkOfflineKey(keyInput))) { keyErr = true; return; }
    settings.offlineEnabled = true;
    keyErr = false;
    keyInput = '';
    keyRow = false;
  }

  // 点“浏览”弹出系统资源管理器的文件夹选择框；取消则保持原值。
  async function browse(key, fallback) {
    const dir = await pickFolder(settings[key] || fallback);
    if (dir) settings[key] = dir;
  }

  const langOptions = $derived(LANGS.map(([v, l]) => [v, v === '' ? t(l) : l]));
  const gameLangOptions = $derived([['auto', t('跟随软件语言')], ...GAME_LANGS]);
  const themeOptions = $derived([['system', t('跟随系统')], ['light', t('浅色')], ['dark', t('深色')]]);
  const afterOptions = $derived([['keep', t('保持不变')], ['minimize', t('最小化启动器，游戏关闭后自动回来')]]);
  const mirrorOptions = $derived([['auto', t('自动')], ['official', t('官方')], ['bmcl', t('镜像')]]);
</script>

<section class="versions" aria-label={t('设置')}>
  <div class="vhead"><h2>{t('设置')}</h2></div>

  <div class="glass vlist setpane" bind:this={panel}>
    <h4>{t('语言')}</h4>
    <div class="rows">
      <div class="srow">
        <b>{t('软件语言')}</b>
        <Select bind:value={settings.locale} options={langOptions} label={t('软件语言')} />
      </div>
      <div class="srow">
        <b>{t('游戏语言')}</b>
        <Select bind:value={settings.gameLang} options={gameLangOptions} label={t('游戏语言')} />
      </div>
    </div>

    <h4>{t('外观')}</h4>
    <div class="rows">
      <div class="srow">
        <b>{t('主题')}</b>
        <Select bind:value={settings.theme} options={themeOptions} label={t('主题')} />
      </div>
      <div class="srow">
        <b>{t('显示青蛙吉祥物')}</b>
        <label class="switch"><input type="checkbox" bind:checked={settings.frog} /><i aria-hidden="true"></i></label>
      </div>
    </div>

    <h4>{t('账号')}</h4>
    <div class="rows">
      {#if settings.offlineEnabled}
        <div class="srow">
          <span class="skinrow">
            <Head size={40} url={activeSkin().url} />
            <span class="lbl"><b>{t('皮肤')}</b><small>{activeSkin().name}</small></span>
          </span>
          <button class="mini" onclick={onSkin}>{t('更换皮肤')}</button>
        </div>
      {/if}
      {#each accounts as a (a.id)}
        <div class="srow">
          <span class="lbl"><b>{a.name}</b><small>{t('微软账号')}</small></span>
          <button class="mini danger" onclick={() => onRemoveAccount(a.id)}>{t('退出登录')}</button>
        </div>
      {/each}
      <div class="srow">
        <span class="lbl"><b>{t('微软正版账号')}</b><small>{t('登录后可以用正版身份进入服务器')}</small></span>
        <button class="mini" onclick={onLogin}>{t('添加账号')}</button>
      </div>
      <div class="srow">
        <span class="lbl"><b>{t('微软登录 Client ID')}</b></span>
        <span class="ctl grow"><input class="txt" bind:value={settings.msClientId} placeholder="00000000-0000-0000-0000-000000000000" spellcheck="false" aria-label={t('微软登录 Client ID')} /></span>
      </div>
    </div>

    <h4>{t('存储位置')}</h4>
    <div class="rows">
      <div class="srow">
        <span class="lbl"><b>{t('游戏下载位置')}</b><small>{t('游戏本体、依赖库、资源和存档放在这里')}</small></span>
        <span class="ctl grow">
          <input class="txt" bind:value={settings.root} placeholder={defaultRoot} spellcheck="false" aria-label={t('游戏下载位置')} />
          {#if inTauri}<button class="mini" onclick={() => browse('root', defaultRoot)}>{t('浏览')}</button>{/if}
          {#if settings.root}<button class="mini" onclick={() => (settings.root = '')}>{t('恢复默认')}</button>{/if}
        </span>
      </div>
      <div class="srow">
        <span class="lbl"><b>{t('FLC 数据位置')}</b><small>{t('Java 运行时、加载器安装器等 FLC 自己的数据放在这里')}</small></span>
        <span class="ctl grow">
          <input class="txt" bind:value={settings.dataDir} placeholder={defaultDataDir} spellcheck="false" aria-label={t('FLC 数据位置')} />
          {#if inTauri}<button class="mini" onclick={() => browse('dataDir', defaultDataDir)}>{t('浏览')}</button>{/if}
          {#if settings.dataDir}<button class="mini" onclick={() => (settings.dataDir = '')}>{t('恢复默认')}</button>{/if}
        </span>
      </div>
    </div>

    <h4>{t('游戏')}</h4>
    <div class="rows">
      <div class="srow">
        <b>{t('内存')}</b>
        <span class="ctl">
          {#if settings.memMode === 'manual'}
            <input class="num" type="number" min="1024" max="32768" step="256" bind:value={settings.memMb} aria-label={t('内存')} /> <em>MB</em>
          {/if}
          <span class="seg">
            <button class:on={settings.memMode === 'auto'} onclick={() => (settings.memMode = 'auto')}>{t('自动')}</button>
            <button class:on={settings.memMode === 'manual'} onclick={() => (settings.memMode = 'manual')}>{t('手动')}</button>
          </span>
        </span>
      </div>
      <div class="srow">
        <span class="lbl"><b>{t('版本隔离')}</b><small>{t('每个版本使用独立的存档、模组与配置')}</small></span>
        <label class="switch"><input type="checkbox" bind:checked={settings.isolate} /><i aria-hidden="true"></i></label>
      </div>
      <div class="srow">
        <b>{t('Java 路径')}</b>
        <span class="ctl grow"><input class="txt" bind:value={settings.java} placeholder={t('自动选择')} spellcheck="false" aria-label={t('Java 路径')} /></span>
      </div>
      <div class="srow">
        <b>{t('启动游戏后')}</b>
        <Select bind:value={settings.afterLaunch} options={afterOptions} label={t('启动游戏后')} />
      </div>
      <div class="srow">
        <b>{t('记录日志')}</b>
        <label class="switch"><input type="checkbox" bind:checked={settings.log} /><i aria-hidden="true"></i></label>
      </div>
    </div>

    <h4>{t('下载源')}</h4>
    <div class="rows">
      <div class="srow">
        <b>{t('下载源')}</b>
        <Select bind:value={settings.mirror} options={mirrorOptions} label={t('下载源')} />
      </div>
    </div>

    <h4>{t('开发者选项')}</h4>
    <div class="rows">
      <div class="srow">
        <b>{t('测试功能')}</b>
        <label class="switch"><input type="checkbox" bind:checked={devOpen} /><i aria-hidden="true"></i></label>
      </div>
      {#if devOpen}
        <div class="srow">
          <b>{t('其他')}</b>
          <em class="sel ver" onclick={tapVersion}>{VERSION}</em>
        </div>
        {#if keyRow}
          <div class="srow">
            <span class="lbl"><b>{t('离线模式')}</b><small>{t('输入密钥后启用')}</small></span>
            <span class="ctl grow">
              <input class="txt" type="password" bind:value={keyInput} spellcheck="false" aria-label={t('离线模式')}
                onkeydown={(e) => e.key === 'Enter' && unlockOffline()} />
              <button class="mini" onclick={unlockOffline}>{t('启用')}</button>
            </span>
          </div>
          {#if keyErr}<p class="err">{t('密钥不正确')}</p>{/if}
        {:else if settings.offlineEnabled}
          <div class="srow">
            <span class="lbl"><b>{t('离线模式')}</b></span>
            <label class="switch"><input type="checkbox" bind:checked={settings.offlineEnabled} /><i aria-hidden="true"></i></label>
          </div>
        {/if}
      {/if}
    </div>

    <h4>{t('关于')}</h4>
    <div class="about">
      <img src="/icon.png" width="64" height="64" alt="" />
      <div class="atxt">
        <b>FLC · Frog Light Core</b>
        <small>{t('一款快、稳、好看的 Minecraft Java 版启动器')}</small>
      </div>
    </div>
    <div class="rows">
      <div class="srow"><b>{t('版本')}</b><em class="sel">{VERSION}</em></div>
      <div class="srow"><b>{t('开发者')}</b><em class="sel">{DEVELOPER}</em></div>
      <div class="srow"><b>{t('开源协议')}</b><em class="sel">{LICENSE}</em></div>
      {#if REPO_URL}
        <div class="srow"><b>{t('项目主页')}</b><button class="mini" onclick={() => call('open_url', { url: REPO_URL })}>{t('在浏览器中打开')}</button></div>
      {/if}
    </div>
    <p class="sfoot">{t('FLC 不是 Mojang Studios 或 Microsoft 的官方产品，与它们没有关联。Minecraft 是 Mojang Studios 的商标。')}</p>
    <p class="sfoot">{t('设置会自动保存，下次启动游戏时生效。')}</p>
  </div>
</section>

<style>
  .skinrow { display: flex; align-items: center; gap: 12px; }
  .skinrow :global(canvas) { border-radius: 9px; }
  .about { display: flex; align-items: center; gap: 16px; padding: 4px 6px 12px; }
  .about img { border-radius: 14px; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12), 0 8px 20px rgba(0, 0, 0, 0.14); }
  .atxt { display: flex; flex-direction: column; gap: 2px; }
  .atxt b { font-size: 17px; font-weight: 600; letter-spacing: -0.01em; }
  .atxt small { color: var(--text-dim); font-size: 12px; }
  em.sel { user-select: text; }
  em.ver { cursor: default; user-select: none; }
  .mini.danger { color: var(--danger); }
</style>
