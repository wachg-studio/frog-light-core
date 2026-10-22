<script>
  let { size = 72, url = '/skins/steve.png' } = $props();
  let canvas;

  // 取皮肤的脸（8,8）并叠上帽子层（40,8），放大时保持像素边缘。
  $effect(() => {
    const src = url;
    const img = new Image();
    img.onload = () => {
      const g = canvas.getContext('2d');
      g.clearRect(0, 0, 8, 8);
      g.imageSmoothingEnabled = false;
      g.drawImage(img, 8, 8, 8, 8, 0, 0, 8, 8);
      g.drawImage(img, 40, 8, 8, 8, 0, 0, 8, 8);
    };
    img.src = src;
  });
</script>

<canvas bind:this={canvas} width="8" height="8" style="width:{size}px;height:{size}px" aria-hidden="true"></canvas>

<style>
  canvas { image-rendering: pixelated; border-radius: 14px; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12), 0 8px 20px rgba(0, 0, 0, 0.12); }
</style>
