<script>
  import { onMount } from 'svelte';
  import {
    WebGLRenderer, Scene, OrthographicCamera, AmbientLight, DirectionalLight, AnimationMixer, Clock,
    LoopOnce, LoopRepeat, DoubleSide, MeshLambertMaterial, MeshBasicMaterial, NearestFilter, SRGBColorSpace,
    TextureLoader, Group, Mesh, PlaneGeometry, CanvasTexture, Raycaster, Vector2,
  } from 'three';
  import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
  import { clone as cloneModel } from 'three/examples/jsm/utils/SkeletonUtils.js';

  // 每个场景一只颜色不同的原版青蛙：站在当前页面主面板的上沿。
  // 切换场景时，旧青蛙跳出画面，新青蛙从上方落下。
  // frog: temperate | cold | warm；anchor: 面板元素；side: 靠左/靠右；mode: idle | busy | running
  let { frog = 'cold', anchor = null, side = 'left', mode = 'idle' } = $props();

  const IDS = ['temperate', 'cold', 'warm'];
  const S = 128; // 每方块对应的像素
  const live = { frog: 'cold', anchor: null, side: 'left', mode: 'idle' };
  $effect(() => {
    live.frog = frog;
    live.anchor = anchor;
    live.side = side;
    live.mode = mode;
  });

  let canvas;

  onMount(() => {
    const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
    const renderer = new WebGLRenderer({ canvas, alpha: true, antialias: true, powerPreference: 'low-power' });
    const scene = new Scene();
    const camera = new OrthographicCamera(0, 1, 0, -1, -1000, 1000);
    scene.add(new AmbientLight(0xffffff, 1.9));
    const sun = new DirectionalLight(0xffffff, 1.6);
    sun.position.set(0.5, 1.2, 1);
    scene.add(sun);

    const clock = new Clock();
    const texLoader = new TextureLoader();
    const frogs = [];
    let raf = 0, alive = true, mx = innerWidth / 2, prevMode = 'idle', lastRender = 0, W = 1, H = 1;

    const loadTex = (id) => texLoader.load(`/frog/${id}.png`, (t) => {
      t.flipY = false; // glTF 的 UV 原点在左上
      t.colorSpace = SRGBColorSpace;
      t.magFilter = t.minFilter = NearestFilter; // 保持像素风
      t.needsUpdate = true;
    });

    // 脚下的柔和投影：面板上有影子，青蛙就不会显得浮在上面。
    const shadowTex = (() => {
      const c = document.createElement('canvas');
      c.width = c.height = 64;
      const g = c.getContext('2d');
      const grad = g.createRadialGradient(32, 32, 0, 32, 32, 32);
      grad.addColorStop(0, 'rgba(0,0,0,1)');
      grad.addColorStop(0.55, 'rgba(0,0,0,0.45)');
      grad.addColorStop(1, 'rgba(0,0,0,0)');
      g.fillStyle = grad;
      g.fillRect(0, 0, 64, 64);
      return new CanvasTexture(c);
    })();
    const shadowGeo = new PlaneGeometry(1, 1);

    // 原版青蛙在陆地上没有待机动画（idle_water 是水里漂浮的姿势，看起来像在游泳），
    // 所以静止时回到骨骼的原始姿势，再叠一点程序化的呼吸。
    const base = () => (live.mode === 'busy' ? 'walk' : 'rest');

    function play(f, name, { once = false, fade = 0.25, speed = 1 } = {}) {
      if (name === 'rest') {
        // 权重淡出到 0，混合器会把骨骼插值回原始姿势
        f.current?.fadeOut(fade);
        f.current = null;
        f.once = false;
        return;
      }
      const a = f.actions[name];
      if (!a) return;
      a.reset();
      a.setLoop(once ? LoopOnce : LoopRepeat, Infinity);
      a.clampWhenFinished = once;
      a.timeScale = speed;
      a.setEffectiveWeight(1); // reset() 不会还原被 fadeOut 压到 0 的权重
      if (f.current && f.current !== a) a.crossFadeFrom(f.current, fade, false);
      else if (!f.current && fade > 0) a.fadeIn(fade);
      a.play();
      f.current = a;
      f.once = once;
    }

    const perch = (r) => ({
      x: live.side === 'left' ? r.left + 90 : r.right - 90,
      y: r.top + 5, // 脚略微陷进面板，看起来是站在上面
    });

    // 新青蛙从上方落到面板上沿。
    function enter(f, now, r) {
      const t = perch(r);
      f.mode = 'in';
      f.x = t.x; f.y = t.y - 360;
      f.landAt = -1e9;
      f.lastSide = live.side;
      if (reduce) { f.y = t.y; f.jump = null; return; }
      f.jump = { from: { x: f.x, y: f.y }, intro: true, started: false, t0: now + 260, dur: 500, arc: 0, lean: 0, to: null };
      play(f, base(), { fade: 0 });
    }

    // 旧青蛙朝最近的一侧跳出画面。
    function leave(f, now) {
      f.mode = 'out';
      const dir = f.x < W / 2 ? -1 : 1;
      const to = { x: dir < 0 ? -170 : W + 170, y: f.y - 14 };
      if (reduce) { f.mode = 'away'; return; }
      const dist = Math.abs(to.x - f.x);
      f.jump = { from: { x: f.x, y: f.y }, intro: false, started: false, t0: now, dur: 600 + Math.min(dist, 900) * 0.2, arc: 70 + Math.min(dist, 900) * 0.1, lean: dir * 0.7, to };
    }

    const ease = (p) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2);

    function step(f, now) {
      const r = live.anchor?.getBoundingClientRect();
      const hasRect = !!r && r.width > 0;
      const wanted = live.frog === f.id;
      if (wanted && hasRect && f.mode === 'away') enter(f, now, r);
      else if (!wanted && f.mode === 'in') leave(f, now);
      if (f.mode === 'away') { f.group.visible = f.shadow.visible = false; return; }

      let tx = f.x, ty = f.y;
      if (f.mode === 'in' && hasRect) { const t = perch(r); tx = t.x; ty = t.y; }

      let air = 0, airMax = 16, sx = 1, sy = 1, lean = 0, visible = true, fade = 1;
      const j = f.jump;
      if (j) {
        const el = now - j.t0;
        if (el < 0) {
          visible = !j.intro; // 落下前先别露面
        } else {
          if (!j.started) { j.started = true; if (!j.intro) play(f, 'jump', { once: true, fade: 0.08 }); }
          const p = Math.min(1, el / j.dur);
          const e = j.intro ? p * p : ease(p);
          const dest = j.to ?? { x: tx, y: ty };
          f.x = j.from.x + (dest.x - j.from.x) * e;
          f.y = j.from.y + (dest.y - j.from.y) * e;
          const s = Math.sin(Math.PI * p);
          air = s * j.arc; airMax = Math.max(1, j.arc);
          sy = 1 + 0.12 * s; sx = 1 - 0.06 * s;
          lean = j.lean * s;
          fade = j.intro ? p : f.mode === 'out' ? 1 - p : 1; // 影子随入场淡入、离场淡出
          if (p >= 1) {
            f.jump = null;
            if (f.mode === 'out') { f.mode = 'away'; f.group.visible = f.shadow.visible = false; return; }
            f.landAt = now;
            play(f, base(), { fade: 0.15 });
          }
        }
      } else {
        f.x = tx; f.y = ty;
        const since = now - f.landAt;
        if (since < 600) { const o = Math.exp(-since / 110) * Math.cos(since / 45); sy = 1 - 0.2 * o; sx = 1 + 0.1 * o; }
        if (live.mode === 'busy') air = Math.max(0, Math.sin(Math.PI * (now / 620 % 1))) * 18;
        else if (!reduce) sy *= 1 + 0.012 * Math.sin(now / 520 + f.id.length); // 呼吸
      }

      const lookYaw = Math.max(-0.5, Math.min(0.5, (mx - f.x) / 500));
      f.yaw += (0.4 + lookYaw + lean - f.yaw) * 0.12;
      f.group.visible = visible;
      f.group.position.set(f.x, -(f.y - air), 0);
      f.group.scale.set(S * sx, S * sy, S * sx);
      f.group.rotation.set(0.3, f.yaw, 0);

      const frac = Math.min(1, air / airMax);
      f.shadow.visible = visible;
      f.shadow.position.set(f.x, -(f.y + 5), -200);
      const w = S * (0.95 - 0.3 * frac);
      f.shadow.scale.set(w, w * 0.34, 1);
      f.shadow.material.opacity = 0.34 * (1 - 0.6 * frac) * fade;
    }

    function fidget(f) {
      f.timer = setTimeout(() => {
        if (f.mode === 'in' && live.mode !== 'busy' && !f.once && !f.jump) play(f, Math.random() < 0.5 ? 'croak' : 'tongue', { once: true });
        fidget(f);
      }, 5000 + Math.random() * 5000);
    }

    new GLTFLoader().load('/frog/frog.glb', (gltf) => {
      if (!alive) return;
      IDS.forEach((id, i) => {
        const model = cloneModel(gltf.scene);
        // 双面渲染：张嘴、吐舌头时能看到方块内侧，而不是穿帮的空洞。
        const mat = new MeshLambertMaterial({ alphaTest: 0.5, map: loadTex(id), side: DoubleSide });
        model.traverse((o) => { if (o.isMesh) { o.material = mat; o.frustumCulled = false; } });
        const group = new Group();
        group.rotation.order = 'YXZ';
        group.add(model);
        group.visible = false;
        scene.add(group);
        const shadow = new Mesh(shadowGeo, new MeshBasicMaterial({ map: shadowTex, transparent: true, depthWrite: false, opacity: 0.3 }));
        shadow.visible = false;
        scene.add(shadow);

        const mixer = new AnimationMixer(model);
        const actions = {};
        for (const key of ['idle', 'walk', 'jump', 'swim', 'croak', 'tongue']) {
          const clip = gltf.animations.find((c) => c.name.includes(key));
          if (clip) actions[key] = mixer.clipAction(clip);
        }
        const f = { id, group, shadow, mixer, actions, current: null, once: false, mode: 'away', x: 0, y: 0, jump: null, lastSide: null, landAt: -1e9, yaw: 0.4, timer: 0 };
        mixer.addEventListener('finished', () => { f.once = false; if (f.mode === 'in' && !f.jump) play(f, base(), { fade: 0.3 }); });
        if (!reduce) setTimeout(() => fidget(f), 1500 + i * 1300);
        frogs.push(f);
      });
    });

    function resize() {
      W = innerWidth; H = innerHeight;
      renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
      renderer.setSize(W, H, false);
      camera.left = 0; camera.right = W; camera.top = 0; camera.bottom = -H;
      camera.updateProjectionMatrix();
    }
    resize();
    addEventListener('resize', resize);

    const onMove = (e) => { mx = e.clientX; };
    addEventListener('pointermove', onMove);

    // 点青蛙：吐舌头。画布本身不拦截鼠标，所以这里用射线自己判断。
    const ray = new Raycaster();
    const onClick = (e) => {
      ray.setFromCamera(new Vector2((e.clientX / W) * 2 - 1, -((e.clientY / H) * 2 - 1)), camera);
      for (const f of frogs) {
        if (f.mode === 'in' && !f.jump && live.mode !== 'busy' && ray.intersectObject(f.group, true).length) {
          play(f, 'tongue', { once: true, fade: 0.1 });
          break;
        }
      }
    };
    addEventListener('click', onClick);

    function frame() {
      raf = requestAnimationFrame(frame);
      const now = performance.now();
      const active = frogs.filter((f) => f.mode !== 'away');
      const moving = active.some((f) => f.jump || now - f.landAt < 700) || (live.mode === 'busy' && active.length > 0);
      if (!moving && now - lastRender < 33) return; // 静止时降到 30 帧，省电
      lastRender = now;
      const dt = Math.min(clock.getDelta(), 0.1);
      if (live.mode !== prevMode) {
        const m = live.mode;
        prevMode = m;
        for (const f of active) {
          if (f.jump || f.mode !== 'in') continue;
          if (m === 'running') play(f, 'croak', { once: true });
          else play(f, m === 'busy' ? 'walk' : 'rest', { fade: 0.3, speed: m === 'busy' ? 1.8 : 1 });
        }
      }
      if (!reduce) active.forEach((f) => f.mixer.update(dt));
      frogs.forEach((f) => step(f, now));
      renderer.render(scene, camera);
    }
    frame();

    const onVis = () => {
      if (document.hidden) cancelAnimationFrame(raf);
      else if (alive) { clock.getDelta(); frame(); }
    };
    document.addEventListener('visibilitychange', onVis);

    return () => {
      alive = false;
      cancelAnimationFrame(raf);
      frogs.forEach((f) => clearTimeout(f.timer));
      removeEventListener('resize', resize);
      removeEventListener('pointermove', onMove);
      removeEventListener('click', onClick);
      document.removeEventListener('visibilitychange', onVis);
      renderer.dispose();
    };
  });
</script>

<canvas bind:this={canvas} aria-hidden="true"></canvas>

<style>
  canvas { position: fixed; inset: 0; width: 100vw; height: 100vh; pointer-events: none; z-index: 20; }
</style>
