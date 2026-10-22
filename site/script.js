(() => {
  const root = document.documentElement;
  root.classList.add('js');

  // 深浅色：记住选择，首次跟随系统
  const saved = localStorage.getItem('flc-theme');
  if (saved === 'light' || saved === 'dark') {
    root.dataset.theme = saved;
  } else if (window.matchMedia('(prefers-color-scheme: light)').matches) {
    root.dataset.theme = 'light';
  }

  const themeBtn = document.getElementById('themeBtn');
  themeBtn?.addEventListener('click', () => {
    const next = root.dataset.theme === 'light' ? 'dark' : 'light';
    root.dataset.theme = next;
    localStorage.setItem('flc-theme', next);
  });

  // 导航滚动态
  const nav = document.getElementById('nav');
  const onScroll = () => nav?.classList.toggle('is-scrolled', window.scrollY > 8);
  onScroll();
  window.addEventListener('scroll', onScroll, { passive: true });

  // 移动端菜单
  const menuBtn = document.getElementById('menuBtn');
  const navLinks = document.getElementById('navLinks');
  const closeMenu = () => {
    navLinks?.classList.remove('open');
    menuBtn?.setAttribute('aria-expanded', 'false');
  };
  menuBtn?.addEventListener('click', () => {
    const open = navLinks?.classList.toggle('open') ?? false;
    menuBtn.setAttribute('aria-expanded', String(open));
  });
  navLinks?.querySelectorAll('a').forEach((a) => a.addEventListener('click', closeMenu));

  // 截图切换
  const shots = {
    home: ['assets/home.png', '主页：账号、存档和一个醒目的“启动游戏”', '主页：账号、当前存档和启动游戏按钮'],
    download: ['assets/download.png', '下载游戏：按系列归类，可顺便装加载器', '下载游戏：按系列归类，可顺便安装加载器'],
    browse: ['assets/browse.png', '模组与资源：Modrinth 搜索，装到哪些存档由你选', '模组与资源：搜索 Modrinth，选择装到哪些存档'],
    skin: ['assets/skin.png', '更换皮肤：3D 预览，预设皮肤或导入自己的', '更换皮肤：3D 预览，使用预设或导入自己的皮肤'],
  };
  const shotImg = document.getElementById('shotImg');
  const shotCap = document.getElementById('shotCap');
  document.querySelectorAll('.shot-tab').forEach((tab) => {
    tab.addEventListener('click', () => {
      const item = shots[tab.dataset.shot];
      if (!item || !shotImg) return;
      document.querySelectorAll('.shot-tab').forEach((t) => {
        const on = t === tab;
        t.classList.toggle('is-on', on);
        t.setAttribute('aria-selected', String(on));
      });
      shotImg.classList.add('is-fading');
      setTimeout(() => {
        shotImg.src = item[0];
        shotImg.alt = item[2];
        if (shotCap) shotCap.textContent = item[1];
        shotImg.classList.remove('is-fading');
      }, 160);
    });
  });

  // 滚动入场
  const revealEls = document.querySelectorAll('.reveal');
  if ('IntersectionObserver' in window) {
    const io = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          entry.target.classList.add('in');
          io.unobserve(entry.target);
        }
      }
    }, { rootMargin: '0px 0px -8% 0px', threshold: 0.08 });
    revealEls.forEach((el) => io.observe(el));
  } else {
    revealEls.forEach((el) => el.classList.add('in'));
  }

  // 卡片跟随鼠标的高光
  document.querySelectorAll('.card').forEach((card) => {
    card.addEventListener('pointermove', (e) => {
      const r = card.getBoundingClientRect();
      card.style.setProperty('--mx', `${e.clientX - r.left}px`);
      card.style.setProperty('--my', `${e.clientY - r.top}px`);
    });
  });
})();
