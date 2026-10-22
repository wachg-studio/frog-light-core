<script>
  import { onMount } from 'svelte';
  import {
    WebGLRenderer, Scene, PerspectiveCamera, AmbientLight, DirectionalLight, Group, Mesh, BoxGeometry,
    MeshLambertMaterial, TextureLoader, NearestFilter, SRGBColorSpace, DoubleSide, BufferAttribute,
  } from 'three';

  // 3D 玩家模型。贴图是标准的 64×64 皮肤；slim 为 true 时手臂 3 像素宽。拖动可以旋转。
  let { url, slim = false, walking = false, autoRotate = true } = $props();

  let wrap;
  let canvas;
  let api; // onMount 里建好的场景句柄

  // 每个面在贴图上的矩形：[x, y, w, h]，对应 BoxGeometry 的 +x -x +y -y +z -z。
  function faceRects(u, v, w, h, d) {
    return [
      [u + d + w, v + d, d, h], // +x：角色的左侧
      [u, v + d, d, h], // -x：角色的右侧
      [u + d, v, w, d], // 顶
      [u + d + w, v, w, d], // 底
      [u + d, v + d, w, h], // 前
      [u + 2 * d + w, v + d, w, h], // 后
    ];
  }

  function boxGeometry(w, h, d, u, v, grow) {
    const g = new BoxGeometry(w + grow * 2, h + grow * 2, d + grow * 2);
    const uv = [];
    for (const [x, y, rw, rh] of faceRects(u, v, w, h, d)) {
      uv.push(x / 64, 1 - y / 64, (x + rw) / 64, 1 - y / 64, x / 64, 1 - (y + rh) / 64, (x + rw) / 64, 1 - (y + rh) / 64);
    }
    g.setAttribute('uv', new BufferAttribute(new Float32Array(uv), 2));
    return g;
  }

  function buildPlayer(isSlim, mats) {
    const player = new Group();
    const aw = isSlim ? 3 : 4;
    // 每个部件 = 以关节为原点的 Group + 里面的内外两层盒子，转动 Group 就是转动关节。
    const part = (name, w, h, d, base, over, joint, centre, grow = 0.25) => {
      const g = new Group();
      g.position.set(...joint);
      const inner = new Mesh(boxGeometry(w, h, d, base[0], base[1], 0), mats.base);
      const outer = new Mesh(boxGeometry(w, h, d, over[0], over[1], grow), mats.over);
      inner.position.set(...centre);
      outer.position.set(...centre);
      g.add(inner, outer);
      g.name = name;
      player.add(g);
      return g;
    };
    part('head', 8, 8, 8, [0, 0], [32, 0], [0, 24, 0], [0, 4, 0], 0.5);
    part('body', 8, 12, 4, [16, 16], [16, 32], [0, 24, 0], [0, -6, 0]);
    part('rightArm', aw, 12, 4, [40, 16], [40, 32], [-4 - aw / 2, 22, 0], [0, -4, 0]);
    part('leftArm', aw, 12, 4, [32, 48], [48, 48], [4 + aw / 2, 22, 0], [0, -4, 0]);
    part('rightLeg', 4, 12, 4, [0, 16], [0, 32], [-2, 12, 0], [0, -6, 0]);
    part('leftLeg', 4, 12, 4, [16, 48], [0, 48], [2, 12, 0], [0, -6, 0]);
    return player;
  }

  onMount(() => {
    const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
    const renderer = new WebGLRenderer({ canvas, alpha: true, antialias: true, powerPreference: 'low-power' });
    const scene = new Scene();
    const camera = new PerspectiveCamera(28, 1, 1, 500);
    camera.position.set(0, 17, 86);
    camera.lookAt(0, 16, 0);
    scene.add(new AmbientLight(0xffffff, 2.1));
    const sun = new DirectionalLight(0xffffff, 1.5);
    sun.position.set(0.6, 1, 1.2);
    scene.add(sun);

    // 不透明的内层用 alphaTest 剔除贴图里的透明像素；外层半透明叠加。
    const mats = {
      base: new MeshLambertMaterial({ transparent: true, alphaTest: 0.5, side: DoubleSide }),
      over: new MeshLambertMaterial({ transparent: true, alphaTest: 0.05, side: DoubleSide }),
    };
    const loader = new TextureLoader();
    let tex = null;
    let player = null;
    let builtSlim = null;
    let yaw = -0.5;
    let dragging = false;
    let lastX = 0;
    let raf = 0;
    let alive = true;
    let t0 = performance.now();

    function setSkin(src, isSlim) {
      if (builtSlim !== isSlim) {
        if (player) {
          scene.remove(player);
          player.traverse((o) => o.geometry?.dispose());
        }
        player = buildPlayer(isSlim, mats);
        player.position.y = -2;
        scene.add(player);
        builtSlim = isSlim;
      }
      loader.load(src, (t) => {
        if (!alive) return t.dispose();
        t.magFilter = t.minFilter = NearestFilter;
        t.colorSpace = SRGBColorSpace;
        t.generateMipmaps = false;
        tex?.dispose();
        tex = t;
        mats.base.map = mats.over.map = t;
        mats.base.needsUpdate = mats.over.needsUpdate = true;
      });
    }

    function resize() {
      const w = wrap.clientWidth;
      const h = wrap.clientHeight;
      if (!w || !h) return;
      renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
      renderer.setSize(w, h, false);
      camera.aspect = w / h;
      // 窗口很窄时拉远一点，保证整个人都在画面里
      camera.position.z = camera.aspect < 0.6 ? 86 / (camera.aspect / 0.6) : 86;
      camera.updateProjectionMatrix();
    }

    const joint = (name) => player?.getObjectByName(name);

    function frame(now) {
      raf = requestAnimationFrame(frame);
      if (!player || document.hidden) return;
      const t = (now - t0) / 1000;
      const cfg = api.cfg;
      if (!dragging && cfg.autoRotate && !reduce) yaw += 0.006;
      player.rotation.y = yaw;
      const swing = cfg.walking ? Math.sin(t * 5) * 0.7 : Math.sin(t * 1.6) * 0.04;
      const sway = cfg.walking ? 1 : 0.5;
      joint('rightArm').rotation.x = swing * sway * 1;
      joint('leftArm').rotation.x = -swing * sway;
      joint('rightLeg').rotation.x = -swing * (cfg.walking ? 1 : 0);
      joint('leftLeg').rotation.x = swing * (cfg.walking ? 1 : 0);
      // 没在走路时手臂略微外展、轻轻呼吸
      joint('rightArm').rotation.z = cfg.walking ? 0 : 0.05 + Math.sin(t * 1.6) * 0.02;
      joint('leftArm').rotation.z = cfg.walking ? 0 : -0.05 - Math.sin(t * 1.6) * 0.02;
      joint('head').rotation.y = cfg.walking ? 0 : Math.sin(t * 0.7) * 0.15;
      renderer.render(scene, camera);
    }

    const ro = new ResizeObserver(resize);
    ro.observe(wrap);
    resize();

    const down = (e) => { dragging = true; lastX = e.clientX; canvas.setPointerCapture(e.pointerId); };
    const move = (e) => { if (dragging) { yaw += (e.clientX - lastX) * 0.012; lastX = e.clientX; } };
    const up = () => { dragging = false; };
    canvas.addEventListener('pointerdown', down);
    canvas.addEventListener('pointermove', move);
    canvas.addEventListener('pointerup', up);
    canvas.addEventListener('pointercancel', up);

    api = { cfg: { walking, autoRotate }, setSkin };
    raf = requestAnimationFrame(frame);

    return () => {
      alive = false;
      cancelAnimationFrame(raf);
      ro.disconnect();
      tex?.dispose();
      mats.base.dispose();
      mats.over.dispose();
      player?.traverse((o) => o.geometry?.dispose());
      renderer.dispose();
    };
  });

  $effect(() => {
    const [u, s] = [url, slim];
    api?.setSkin(u, s);
  });
  $effect(() => {
    const cfg = { walking, autoRotate };
    if (api) api.cfg = cfg;
  });
</script>

<div class="viewer" bind:this={wrap}>
  <canvas bind:this={canvas} aria-label="skin preview"></canvas>
</div>

<style>
  .viewer { width: 100%; height: 100%; min-height: 0; }
  canvas { width: 100%; height: 100%; display: block; cursor: grab; touch-action: none; }
  canvas:active { cursor: grabbing; }
</style>
