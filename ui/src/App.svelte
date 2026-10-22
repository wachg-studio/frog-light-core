<script>
  import { onMount, untrack } from 'svelte';
  import { call, onEvent, windowControl, restoreWindow, setWindowTheme, inTauri, CANCELLED } from './api.js';
  import { diagnose } from './diagnose.js';
  import Ring from './Ring.svelte';
  import Head from './Head.svelte';
  import Settings from './Settings.svelte';
  import Download from './Download.svelte';
  import Instances from './Instances.svelte';
  import Browse from './Browse.svelte';
  import Select from './Select.svelte';
  import TaskPanel from './TaskPanel.svelte';
  import LoginDialog from './LoginDialog.svelte';
  import { LOADER_NAMES } from './loaders.js';
  import { settings, save } from './settings.svelte.js';
  import { activeSkin, skinPayload } from './skins.svelte.js';
  import { t, locale, autoGameLang, loadLocale } from './i18n.svelte.js';

  // three.js 约 170 KB（gzip），懒加载，不挡住首屏。
  const loadFrogs = import('./FrogStage.svelte');
  // 皮肤弹窗带着 3D 预览（同样依赖 three.js），点开时才加载。
  const loadSkinDialog = () => import('./SkinDialog.svelte');
  // 每个场景一只不同颜色的原版青蛙（温带=橙，寒带=绿，热带=白）。
  const SCENE_FROG = { home: 'cold', versions: 'temperate', download: 'warm', browse: 'temperate', settings: 'cold' };
  const SIDE = { home: 'left' };

  const store = {
    get: (k, d) => { try { return localStorage.getItem(k) ?? d; } catch { return d; } },
    set: (k, v) => { try { localStorage.setItem(k, v); } catch {} },
  };

  /** 随每次后端调用一起发过去的设置。 */
  function bs() {
    return {
      root: settings.root,
      dataDir: settings.dataDir,
      mirror: settings.mirror,
      isolate: settings.isolate,
      memMb: settings.memMode === 'manual' ? Number(settings.memMb) || null : null,
      java: settings.java,
      lang: settings.gameLang === 'auto' ? autoGameLang() : settings.gameLang,
      msClientId: settings.msClientId,
    };
  }

  let page = $state('home');
  // 每个页面各有一个主面板，青蛙停在当前页面面板的上沿。页面都常驻（切走只是隐藏），所以要分开记。
  let panels = $state({ home: null, versions: null, browse: null, download: null, settings: null });
  const panelEl = $derived(panels[page]);
  // 页面第一次打开才创建，之后一直保留：切到别的页面再回来，停在哪儿、选了什么都还在
  let visited = $state({ home: true });
  let instances = $state([]);
  let remote = $state([]);
  let remoteState = $state('idle'); // idle | loading | error
  let selected = $state(store.get('flc.version', ''));
  let name = $state(store.get('flc.name', 'Player'));
  let defaultRoot = $state('');
  let defaultDataDir = $state('');

  // 启动
  let phase = $state('idle'); // idle | busy | running
  let ready = $state(false); // 游戏资源已加载完，窗口已出现
  let tip = $state(null); // { k, v }：提示文案的 key 与参数，显示时再翻译，切换语言能跟着变
  let crash = $state(false); // 异常退出后，日志面板里显示的是最后几十行输出
  let error = $state('');
  // 下载
  let dlError = $state('');
  let notice = $state(null);
  // 两者共用的进度
  let stage = $state('');
  let detail = $state('');
  let ring = $state(0);
  let prepareMs = $state(null);
  let logs = $state([]);

  let killed = false;
  let timers = [];
  const clearTimers = () => { timers.forEach(clearTimeout); timers = []; };

  // ---- 后台任务：排队、取消、详情 ----
  let current = $state(null); // 正在进行的下载 / 安装：{ id, label, content }
  let queue = $state([]); // 排队中的：{ id, label, content, run, done }
  let history = $state([]); // 最近几个任务的结果
  let taskOpen = $state(false);
  let files = $state([]); // 此刻正在下载的几个文件
  let cancelling = $state(false);
  let seq = 0;
  let flash = $state(null); // 任务刚结束时，右上角短暂显示结果：{ label, ok }
  let flashTimer;
  const dlId = $derived(current?.label ?? '');
  const queued = $derived(queue.map((q) => q.label));
  const working = $derived(phase !== 'idle'); // 正在启动 / 运行游戏
  const launchBlocked = $derived(phase !== 'idle' || current !== null || queue.length > 0);
  // 右上角的全局进度：不管在哪个页面，只要在下载或准备游戏，就显示正在处理什么和进度
  const task = $derived(
    current
      ? { verb: '正在下载', label: current.label }
      : phase === 'busy'
        ? { verb: '正在准备', label: selectedInst?.title ?? selected }
        : queue.length
          ? { verb: '排队中', label: queue[0].label }
          : null,
  );
  const frogMode = $derived(current ? 'busy' : phase);

  const STAGE_RING = { 获取版本清单: 0.03, 解析版本: 0.05, 检查文件: 0.1, '下载 Java': 0.1, 准备运行环境: 0.93, 启动游戏: 0.97, 进程已创建: 0.99, 写入加载器配置: 0.94, 运行安装器: 0.6 };
  const mb = (n) => (n / 1e6).toFixed(n > 1e8 ? 0 : 1);
  const installed = $derived(instances.map((i) => i.id));
  const selectedInst = $derived(instances.find((i) => i.id === selected));
  const isInstalled = (id) => installed.includes(id);
  let skinOpen = $state(false);
  let browseKind = $state(store.get('flc.browseKind', 'mod'));
  let browseTarget = $state('*'); // “安装到”默认是全部存档
  const skin = $derived(activeSkin());

  function onStarted() {
    phase = 'running';
    ready = false;
    ring = 1;
    stage = '游戏已启动';
    detail = prepareMs != null ? t('点击到进程创建 {ms} ms', { ms: prepareMs }) : '';
    tip = { k: '游戏进程已经启动，窗口还在加载，请稍等一会儿。' };
    clearTimers();
    timers.push(setTimeout(() => { if (!ready && phase === 'running') tip = { k: '加载比较慢：首次启动、资源多或装了模组时，可能要一两分钟。窗口出现前请不要重复点击。' }; }, 15000));
    timers.push(setTimeout(() => { if (!ready && phase === 'running') tip = { k: '已经等了一分钟还没出现窗口，可能卡住了。打开日志看看有没有报错，或点“结束游戏”后重试。' }; }, 60000));
    refreshInstalled();
  }

  let autoMinimized = false; // 游戏窗口出现后是不是我们帮用户最小化的（游戏关闭后要还原）

  function onReady() {
    ready = true;
    if (settings.afterLaunch === 'minimize') {
      autoMinimized = true;
      windowControl('minimize');
    }
    clearTimers();
    tip = { k: '游戏窗口已经出现。' };
    timers.push(setTimeout(() => { if (ready) tip = null; }, 5000));
  }

  function onExited(e) {
    clearTimers();
    if (autoMinimized) {
      autoMinimized = false;
      restoreWindow().catch(() => {});
    }
    phase = 'idle';
    ring = 0;
    stage = '';
    detail = '';
    if (e.code === 0 || killed) {
      // 正常关闭或手动结束：日志用完即清，下次启动是干净的。
      logs = [];
      crash = false;
      tip = null;
    } else {
      logs = e.tail ?? logs;
      crash = true;
      const code = e.code ?? t('未知');
      // 先尝试从输出里认出具体原因；认不出来再给通用的提示
      tip = diagnose(e.tail ?? logs) ?? (!e.ready && e.secs < 30
        ? { k: '游戏启动后立刻退出了（退出码 {code}）。常见原因：Java 版本不匹配、内存不足、模组冲突或文件损坏。右侧是最后的输出。', v: { code } }
        : { k: '游戏异常退出了（退出码 {code}），右侧是最后的输出。', v: { code } });
    }
    killed = false;
  }

  function handle(e) {
    if (e.type === 'stage') {
      stage = e.name;
      ring = Math.max(ring, STAGE_RING[e.name.replace(' ', '')] ?? STAGE_RING[e.name] ?? ring);
      if (e.name === '进程已创建') prepareMs = e.ms;
    } else if (e.type === 'progress') {
      detail = e.total > 0 ? `${t(e.label)}  ${mb(e.done)} / ${mb(e.total)} MB` : `${t(e.label)}  ${e.filesDone} / ${e.filesTotal}`;
      files = e.active ?? [];
      if (current?.content && e.total > 0) ring = Math.max(0.02, e.done / e.total);
      else if (e.label === '游戏文件' && e.total > 0) ring = Math.max(ring, 0.1 + 0.8 * (e.done / e.total));
    } else if (e.type === 'log') {
      if (settings.log) logs = [...logs.slice(-299), e.line];
    } else if (e.type === 'started') onStarted();
    else if (e.type === 'ready') onReady();
    else if (e.type === 'exited') onExited(e);
  }

  async function refreshInstalled() {
    instances = await call('list_instances', { settings: bs() });
    if (!instances.some((i) => i.id === selected)) selected = instances[0]?.id ?? '';
  }

  async function loadRemote() {
    if (remoteState === 'loading' || remote.length) return;
    remoteState = 'loading';
    try {
      remote = await call('list_remote', { settings: bs() });
      remoteState = 'idle';
    } catch (e) {
      remoteState = 'error';
      dlError = String(e);
    }
  }

  function choose(id) {
    selected = id;
    store.set('flc.version', id);
    page = 'home';
  }

  async function launch() {
    if (!selected || launchBlocked) return;
    if (settings.accountType === 'msa' && !settings.accountId) { loginOpen = true; return; }
    store.set('flc.name', name);
    phase = 'busy';
    ring = 0.02;
    stage = '准备中';
    detail = '';
    files = [];
    prepareMs = null;
    error = '';
    tip = null;
    crash = false;
    killed = false;
    logs = [];
    const msa = settings.accountType === 'msa';
    try {
      await call('launch_game', { id: selected, name, account: msa ? settings.accountId : null, skin: msa ? null : await skinPayload(), settings: bs() });
    } catch (e) {
      phase = 'idle';
      ring = 0;
      if (String(e) !== CANCELLED) error = String(e);
    } finally {
      cancelling = false;
    }
  }

  /**
   * 把一个下载 / 安装放进队列，按顺序一个个做。返回 { ok, value } 或 { ok: false, cancelled?, error }；
   * 同名任务已经在做或在排队时返回 null。run 收到 { isCancelled() }，多步任务可以在步骤之间检查。
   */
  function enqueue(label, run, { content = false } = {}) {
    if (current?.label === label || queue.some((q) => q.label === label)) return Promise.resolve(null);
    return new Promise((done) => {
      queue.push({ id: ++seq, label, content, run, done });
      pump();
    });
  }

  async function pump() {
    if (current || phase === 'busy' || !queue.length) return; // 正在准备游戏时先等一等
    const job = queue.shift();
    current = { id: job.id, label: job.label, content: job.content };
    ring = 0.02;
    stage = '准备中';
    detail = '';
    files = [];
    cancelling = false;
    let result;
    try {
      result = { ok: true, value: await job.run({ isCancelled: () => cancelling }) };
    } catch (e) {
      const text = String(e);
      result = text === CANCELLED ? { ok: false, cancelled: true, error: text } : { ok: false, error: text };
    }
    history = [{ id: job.id, label: job.label, status: result.ok ? 'ok' : result.cancelled ? 'cancelled' : 'err', text: result.ok ? '' : result.error }, ...history].slice(0, 5);
    current = null;
    ring = 0;
    stage = '';
    detail = '';
    files = [];
    cancelling = false;
    if (!result.cancelled) {
      flash = { label: job.label, ok: result.ok };
      clearTimeout(flashTimer);
      flashTimer = setTimeout(() => (flash = null), 3200);
    }
    job.done(result);
    pump();
  }

  /** 取消正在进行的任务（下载或准备游戏）。 */
  async function cancelCurrent() {
    if (cancelling) return;
    cancelling = true;
    try { await call('cancel_task'); } catch { /* 任务可能刚好结束 */ }
  }

  function dropQueued(id) {
    const i = queue.findIndex((q) => q.id === id);
    if (i < 0) return;
    const [q] = queue.splice(i, 1);
    history = [{ id: q.id, label: q.label, status: 'cancelled', text: '' }, ...history].slice(0, 5);
    q.done({ ok: false, cancelled: true, error: CANCELLED });
  }

  function cancelAll() {
    for (const q of [...queue]) dropQueued(q.id);
    if (current || phase === 'busy') cancelCurrent();
  }

  /** 去某个存档的设置页（装了整合包之后用）。 */
  function openInstance(id) {
    selected = id;
    store.set('flc.version', id);
    page = 'versions';
  }

  /** 从存档设置页跳到下载页，预选类型与目标存档。 */
  function browseFor(kind, id) {
    browseKind = kind;
    browseTarget = id;
    page = 'browse';
  }

  /** 删除一个存档；成功返回空串，失败返回错误信息。 */
  async function removeVersion(id) {
    try {
      await call('delete_instance', { id, settings: bs() });
      await refreshInstalled();
      if (selected === id) store.set('flc.version', selected);
      return '';
    } catch (e) {
      return String(e);
    }
  }
  let logCopied = $state(false);
  async function copyLogs() {
    try {
      await navigator.clipboard.writeText(logs.join('\n'));
      logCopied = true;
      setTimeout(() => (logCopied = false), 1500);
    } catch {
      // 剪贴板不可用就算了
    }
  }

  async function stopGame() {
    killed = true;
    await call('kill_game');
  }

  /** 下载一个版本；loader 为 { kind, version } 时同时安装模组加载器，装好后把它设为当前版本。 */
  async function download(mc, loader) {
    const label = loader ? `${mc} + ${LOADER_NAMES[loader.kind] ?? loader.kind} ${loader.version}` : mc;
    dlError = '';
    notice = null;
    const res = await enqueue(label, async () => {
      const id = await call('install_game', { id: mc, loader, settings: bs() });
      await refreshInstalled();
      return id;
    });
    if (!res) { notice = { k: '已经在下载队列里了' }; return; }
    if (res.ok) {
      notice = { k: '{id} 已下载完成。', v: { id: res.value } };
      selected = res.value;
      store.set('flc.version', res.value);
    } else if (res.cancelled) {
      notice = { k: '已取消下载 {id}', v: { id: label } };
    } else {
      dlError = res.error;
    }
  }

  // ---- 正版账号 ----
  let accounts = $state([]);
  let loginOpen = $state(false);
  let msSkin = $state(null); // 当前正版账号在 Mojang 那边的皮肤 { url, slim }
  const account = $derived(accounts.find((a) => a.id === settings.accountId));
  const isMsa = $derived(settings.accountType === 'msa');
  const headUrl = $derived(isMsa ? (msSkin?.url ?? '/skins/steve.png') : skin.url);

  async function refreshAccounts() {
    try { accounts = await call('list_accounts', { settings: bs() }); } catch { accounts = []; }
    if (!accounts.some((a) => a.id === settings.accountId)) settings.accountId = accounts[0]?.id ?? '';
  }
  async function removeAccount(id) {
    try { await call('remove_account', { id, settings: bs() }); } catch { /* 删不掉就算了，下次刷新会显示 */ }
    await refreshAccounts();
  }
  function onLoggedIn(info) {
    loginOpen = false;
    settings.accountType = 'msa';
    settings.accountId = info.id;
    refreshAccounts();
  }
  function onHeadClick() {
    if (isMsa) call('open_url', { url: 'https://www.minecraft.net/msaprofile/mygames/editskin' }).catch(() => {});
    else skinOpen = true;
  }
  $effect(() => {
    const [type, id] = [settings.accountType, settings.accountId];
    msSkin = null;
    if (type !== 'msa' || !id) return;
    call('account_skin', { id, settings: bs() }).then((r) => { if (settings.accountId === id) msSkin = r; }).catch(() => {});
  });

  onMount(() => {
    if (!inTauri) document.documentElement.classList.add('web-preview');
    let off = () => {};
    onEvent(handle).then((f) => (off = f));
    call('default_root').then((p) => (defaultRoot = p));
    call('default_data_dir').then((p) => (defaultDataDir = p));
    refreshInstalled();
    refreshAccounts();

    // 这是一个应用，不是网页：去掉浏览器自带的右键菜单（返回/刷新/另存为/打印…）和对应快捷键。
    // 文本框里保留系统的编辑菜单（剪切/复制/粘贴），方便粘贴路径。
    const noMenu = (e) => { if (!e.target.closest?.('input, textarea')) e.preventDefault(); };
    const noKeys = (e) => {
      const k = e.key.toLowerCase();
      const blocked = e.key === 'F5' || e.key === 'F12' || (e.ctrlKey && ['r', 'p', 's', 'u', 'f', 'g', 'j'].includes(k)) || (e.ctrlKey && e.shiftKey && ['i', 'c', 'r'].includes(k)) || (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight'));
      if (blocked) e.preventDefault();
    };
    const noZoom = (e) => { if (e.ctrlKey) e.preventDefault(); };
    addEventListener('contextmenu', noMenu);
    addEventListener('keydown', noKeys);
    addEventListener('wheel', noZoom, { passive: false });
    return () => {
      off(); clearTimers();
      removeEventListener('contextmenu', noMenu);
      removeEventListener('keydown', noKeys);
      removeEventListener('wheel', noZoom);
    };
  });

  // 主题：写到 <html data-theme> 让样式生效，并让窗口材质（Mica）一起换
  $effect(() => {
    const th = settings.theme;
    const root = document.documentElement;
    if (th === 'light' || th === 'dark') root.dataset.theme = th;
    else delete root.dataset.theme;
    setWindowTheme(th === 'system' ? null : th).catch(() => {});
  });

  $effect(() => { $state.snapshot(settings); save(); }); // 设置自动保存
  $effect(() => { document.documentElement.lang = locale(); loadLocale(); });
  $effect(() => { settings.root; refreshInstalled(); }); // 换了游戏目录就重新扫描已安装版本
  $effect(() => { settings.mirror; remote = []; remoteState = 'idle'; }); // 换了下载源就重新取清单
  $effect(() => { if (page === 'download') loadRemote(); });
  $effect(() => { visited[page] = true; });
  $effect(() => { store.set('flc.browseKind', browseKind); });
  $effect(() => { void phase; untrack(pump); }); // 游戏准备完了，接着做排队的任务
  $effect(() => { if (!task && !history.length) taskOpen = false; });
</script>

<div class="shell">
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>FLC</div>
    {#if task || history.length}
      <button class="task" class:idle={!task && !flash} class:ok={!task && flash?.ok} class:bad={!task && flash && !flash.ok} onclick={() => (taskOpen = !taskOpen)} aria-expanded={taskOpen} title={t('查看下载任务')}>
        {#if task}
          <Ring value={ring} size={24} stroke={3} />
          <span class="ttext">
            <b>{t(task.verb)} {task.label}</b>
            <small>{task.verb === '排队中' ? t('等待开始') : `${Math.round(ring * 100)}%${detail ? ` · ${detail}` : ''}`}</small>
          </span>
          {#if queue.length && current}<span class="more" title={t('排队中')}>+{queue.length}</span>{/if}
        {:else if flash}
          <span class="tmark" aria-hidden="true">{flash.ok ? '✓' : '!'}</span>
          <span class="ttext"><b>{flash.ok ? t('完成') : t('失败')} · {flash.label}</b></span>
        {:else}
          <span class="ttext"><b>{t('最近的下载任务')}</b></span>
        {/if}
      </button>
    {/if}
    <div class="controls">
      <button aria-label={t('最小化')} onclick={() => windowControl('minimize')}><svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" stroke-width="1.2" /></svg></button>
      <button aria-label={t('最大化')} onclick={() => windowControl('toggle')}><svg width="10" height="10" viewBox="0 0 10 10"><rect x="1.5" y="1.5" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" /></svg></button>
      <button class="close" aria-label={t('关闭')} onclick={() => windowControl('close')}><svg width="10" height="10" viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" stroke-width="1.2" /></svg></button>
    </div>
  </header>

  <nav class="side">
    <button class:active={page === 'home'} aria-label={t('主页')} title={t('主页')} onclick={() => (page = 'home')}>
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 11l9-8 9 8" /><path d="M5 10v10h5v-6h4v6h5V10" /></svg>
    </button>
    <button class:active={page === 'versions'} aria-label={t('我的存档')} title={t('我的存档')} onclick={() => (page = 'versions')}>
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l9 5-9 5-9-5 9-5z" /><path d="M3 13l9 5 9-5" /><path d="M3 17.5l9 5 9-5" opacity=".5" /></svg>
    </button>
    <button class:active={page === 'download'} aria-label={t('下载游戏')} title={t('下载游戏')} onclick={() => (page = 'download')}>
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11" /><path d="M7 11l5 5 5-5" /><path d="M5 20h14" /></svg>
    </button>
    <button class:active={page === 'browse'} aria-label={t('模组与资源')} title={t('模组与资源')} onclick={() => (page = 'browse')}>
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3z" /><path d="M12 12l8-4.5M12 12v9M12 12L4 7.5" /></svg>
    </button>
    <span class="grow"></span>
    <button class:active={page === 'settings'} aria-label={t('设置')} title={t('设置')} onclick={() => (page = 'settings')}>
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h9" /><path d="M17 7h3" /><circle cx="15" cy="7" r="2" /><path d="M4 17h3" /><path d="M11 17h9" /><circle cx="9" cy="17" r="2" /></svg>
    </button>
  </nav>

  <main>
    {#if visited.home}
    <div class="pg" class:off={page !== 'home'}>
      <section class="home" aria-label={t('主页')}>
        <aside class="glass card">
          <div class="seg acct" role="tablist" aria-label={t('账号类型')}>
            <button role="tab" class:on={!isMsa} aria-selected={!isMsa} disabled={phase !== 'idle'} onclick={() => (settings.accountType = 'offline')}>{t('离线')}</button>
            <button role="tab" class:on={isMsa} aria-selected={isMsa} disabled={phase !== 'idle'} onclick={() => { settings.accountType = 'msa'; if (!accounts.length) loginOpen = true; }}>{t('正版')}</button>
          </div>

          <div class="who">
            <button class="headbtn" onclick={onHeadClick} aria-label={isMsa ? t('在官网修改皮肤') : t('更换皮肤')} title={isMsa ? t('正版账号的皮肤在 minecraft.net 修改') : t('更换皮肤')}>
              <Head size={72} url={headUrl} />
              <span class="hint">{isMsa ? t('在官网修改皮肤') : t('更换皮肤')}</span>
            </button>
            {#if isMsa}
              {#if account}
                <div class="acctbox">
                  <b class="aname">{account.name}</b>
                  {#if accounts.length > 1}
                    <Select bind:value={settings.accountId} options={accounts.map((a) => [a.id, a.name])} label={t('选择账号')} />
                  {/if}
                  <span class="aacts">
                    <button class="mini" disabled={phase !== 'idle'} onclick={() => (loginOpen = true)}>{t('添加账号')}</button>
                    <button class="mini danger" disabled={phase !== 'idle'} onclick={() => removeAccount(account.id)}>{t('退出登录')}</button>
                  </span>
                </div>
              {:else}
                <button class="ghost loginbtn" onclick={() => (loginOpen = true)}>{t('登录微软账号')}</button>
              {/if}
            {:else}
              <label class="field name">
                <input bind:value={name} disabled={phase !== 'idle'} maxlength="16" spellcheck="false" placeholder={t('玩家名')} aria-label={t('玩家名')} />
              </label>
            {/if}
          </div>

          <div class="actions">
            {#if tip}<p class="tip" class:warn={crash}>{t(tip.k, tip.v)}</p>{/if}
            {#if phase !== 'idle'}
              <div class="progress">
                <Ring value={ring} size={46}>
                  {#if phase === 'running' && ready}
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
                  {:else if phase === 'running'}
                    <span class="spin" aria-hidden="true"></span>
                  {:else}
                    <span class="pct">{Math.round(ring * 100)}</span>
                  {/if}
                </Ring>
                <div class="ptext">
                  <b>{phase === 'running' ? (ready ? t('游戏运行中') : t('游戏加载中')) : t(stage)}</b>
                  <span>{detail || ' '}</span>
                </div>
              </div>
            {:else}
              <button class="launch" disabled={!selected || (isMsa && !account) || launchBlocked} onclick={launch}>
                <b>{t('启动游戏')}</b>
                <small>{selected ? (isInstalled(selected) ? (selectedInst?.title ?? selected) : t('{id} · 将自动下载', { id: selected })) : t('请先选择或下载版本')}</small>
              </button>
              {#if error}<p class="err">{error}</p>{/if}
            {/if}
            <div class="two">
              <button class="ghost" onclick={() => (page = 'versions')}>{t('切换存档')}</button>
              {#if phase === 'running'}
                <button class="ghost danger" onclick={stopGame}>{t('结束游戏')}</button>
              {/if}
            </div>
          </div>
        </aside>

        <div class="glass logpane" bind:this={panels.home}>
          <div class="lhead">
            <h3>{t('游戏日志')}</h3>
            <span class="lacts">
              <button class="mini" disabled={!logs.length} onclick={copyLogs}>{logCopied ? t('已复制') : t('复制日志')}</button>
            <label class="switch" title={t('关闭后不再记录游戏输出（异常退出时仍会保留最后几十行用来排查）')}>
              <span>{t('记录日志')}</span>
              <input type="checkbox" bind:checked={settings.log} />
              <i aria-hidden="true"></i>
            </label>
            </span>
          </div>
          <pre>{logs.length ? logs.join('\n') : settings.log ? t('启动后，游戏输出会显示在这里；游戏关闭后自动清空。') : t('日志记录已关闭。')}</pre>
        </div>
      </section>
    </div>
    {/if}
    {#if visited.versions}
    <div class="pg" class:off={page !== 'versions'}>
      <Instances
        bind:panel={panels.versions}
        {instances}
        {selected}
        settings={bs}
        onChoose={choose}
        onChanged={refreshInstalled}
        onDelete={removeVersion}
        onDownload={() => (page = 'download')}
        onBrowse={browseFor}
        onRun={(label, fn) => enqueue(label, fn, { content: true })}
      />
    </div>
    {/if}
    {#if visited.browse}
    <div class="pg" class:off={page !== 'browse'}>
      <Browse
        bind:panel={panels.browse}
        bind:kind={browseKind}
        bind:target={browseTarget}
        {instances}
        settings={bs}
        {working}
        {ring}
        onRun={(label, fn) => enqueue(label, fn, { content: true })}
        current={dlId}
        {queued}
        onChanged={refreshInstalled}
        onOpenInstance={openInstance}
        onGoDownload={() => (page = 'download')}
      />
    </div>
    {/if}
    {#if visited.download}
    <div class="pg" class:off={page !== 'download'}>
      <Download
        bind:panel={panels.download}
        {remote}
        {remoteState}
        {installed}
        {dlId}
        {queued}
        {ring}
        {detail}
        working={phase === 'busy'}
        {notice}
        {dlError}
        settings={bs}
        onInstall={download}
        onDelete={removeVersion}
      />
    </div>
    {/if}
    {#if visited.settings}
    <div class="pg" class:off={page !== 'settings'}>
      <Settings bind:panel={panels.settings} {defaultRoot} {defaultDataDir} {accounts} onSkin={() => (skinOpen = true)} onLogin={() => (loginOpen = true)} onRemoveAccount={removeAccount} />
    </div>
    {/if}
  </main>

  {#if taskOpen && (task || history.length)}
    <TaskPanel
      {current}
      launching={!current && phase === 'busy' ? { label: selectedInst?.title ?? selected } : null}
      {queue}
      {history}
      {ring}
      {stage}
      {detail}
      {files}
      {cancelling}
      onCancel={cancelCurrent}
      onDrop={dropQueued}
      onCancelAll={cancelAll}
      onClose={() => (taskOpen = false)}
    />
  {/if}

  {#if loginOpen}
    <LoginDialog settings={bs} onDone={onLoggedIn} onClose={() => (loginOpen = false)} onSettings={() => (page = 'settings')} />
  {/if}

  {#if skinOpen}
    {#await loadSkinDialog() then { default: SkinDialog }}
      <SkinDialog onClose={() => (skinOpen = false)} />
    {/await}
  {/if}

  {#if settings.frog}
    {#await loadFrogs then { default: FrogStage }}
      <FrogStage frog={SCENE_FROG[page]} anchor={panelEl} side={SIDE[page] ?? 'right'} mode={frogMode} />
    {/await}
  {/if}
</div>
