<script lang="ts">
  /**
   * Canvas-based lava lamp gradient background.
   * Four colored ellipses with Gaussian blur following Lissajous motion curves.
   * Renders at 60fps via requestAnimationFrame.
   */

  let { energy = 0.5, visible = true }: { energy?: number; visible?: boolean } = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let animationId: number = 0;
  let startTime: number = 0;

  // Blob definitions — colors, sizes, and Lissajous parameters
  const blobs = [
    {
      color: [147, 51, 234],  // purple
      width: 300,
      height: 140,
      xFreq: 0.7, yFreq: 0.5,
      xAmp: 120, yAmp: 35,
      xPhase: 0, yPhase: 0,
      brightnessScale: 1.0,
    },
    {
      color: [59, 130, 246],  // blue
      width: 360,
      height: 160,
      xFreq: 0.6, yFreq: 0.45,
      xAmp: 140, yAmp: 40,
      xPhase: 1.5, yPhase: 1.0,
      brightnessScale: 0.9,
      xUseSin: true,
    },
    {
      color: [34, 211, 238],  // cyan
      width: 280,
      height: 120,
      xFreq: 0.8, yFreq: 0.6,
      xAmp: 100, yAmp: 30,
      xPhase: 3.0, yPhase: 2.0,
      brightnessScale: 0.85,
    },
    {
      color: [99, 102, 241],  // indigo
      width: 320,
      height: 130,
      xFreq: 0.55, yFreq: 0.7,
      xAmp: 130, yAmp: 35,
      xPhase: 4.5, yPhase: 3.5,
      brightnessScale: 0.9,
      xUseSin: true,
    },
  ];

  function render(ctx: CanvasRenderingContext2D, t: number) {
    const w = ctx.canvas.width;
    const h = ctx.canvas.height;
    const cx = w / 2;
    const cy = h / 2;

    ctx.clearRect(0, 0, w, h);

    // Clip to pill shape so gradient doesn't bleed outside on transparent windows
    ctx.save();
    const dpr = window.devicePixelRatio || 1;
    const pillW = w / dpr;
    const pillH = h / dpr;
    const radius = pillH / 2; // pill shape = half-height radius
    ctx.beginPath();
    ctx.roundRect(0, 0, pillW, pillH, radius);
    ctx.clip();

    const speed = 0.4 + energy * 0.6;
    const brightness = 0.25 + energy * 0.25;

    for (const blob of blobs) {
      const blobBrightness = brightness * blob.brightnessScale;

      // Lissajous motion
      const xTrigFn = blob.xUseSin ? Math.sin : Math.cos;
      const yTrigFn = blob.xUseSin ? Math.cos : Math.sin;

      const x = cx + xTrigFn(t * blob.xFreq * speed + blob.xPhase) * blob.xAmp;
      const y = cy + yTrigFn(t * blob.yFreq * speed + blob.yPhase) * blob.yAmp;

      // Draw ellipse with radial gradient for soft edges
      const rx = blob.width / 2;
      const ry = blob.height / 2;
      const maxR = Math.max(rx, ry);

      const gradient = ctx.createRadialGradient(x, y, 0, x, y, maxR);
      const [r, g, b] = blob.color;
      gradient.addColorStop(0, `rgba(${r}, ${g}, ${b}, ${blobBrightness})`);
      gradient.addColorStop(0.4, `rgba(${r}, ${g}, ${b}, ${blobBrightness * 0.6})`);
      gradient.addColorStop(1, `rgba(${r}, ${g}, ${b}, 0)`);

      ctx.save();
      ctx.translate(x, y);
      ctx.scale(rx / maxR, ry / maxR);
      ctx.translate(-x, -y);

      ctx.fillStyle = gradient;
      ctx.beginPath();
      ctx.arc(x, y, maxR, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }
    ctx.restore(); // restore the pill clip
  }

  function loop(timestamp: number) {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    if (!startTime) startTime = timestamp;
    const t = (timestamp - startTime) / 1000;

    render(ctx, t);
    animationId = requestAnimationFrame(loop);
  }

  function startAnimation() {
    if (!canvas) return;

    // Set canvas size to match display
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;

    const ctx = canvas.getContext('2d');
    if (ctx) {
      ctx.scale(dpr, dpr);
      // Apply Gaussian blur — reduced from 55px since canvas is clipped to pill shape
      ctx.filter = 'blur(30px)';
    }

    startTime = 0;
    animationId = requestAnimationFrame(loop);
  }

  function stopAnimation() {
    if (animationId) {
      cancelAnimationFrame(animationId);
      animationId = 0;
    }
  }

  $effect(() => {
    if (visible && canvas) {
      startAnimation();
    } else {
      stopAnimation();
    }

    return () => {
      stopAnimation();
    };
  });
</script>

<div
  class="lava-lamp-wrapper"
  style="opacity: {visible ? 1 : 0}; transition: opacity 800ms ease-in-out;"
>
  <canvas bind:this={canvas}></canvas>
</div>

<style>
  .lava-lamp-wrapper {
    position: absolute;
    inset: 0;
    pointer-events: none;
    overflow: hidden;
    border-radius: inherit; /* inherit pill's border-radius to clip */
  }

  .lava-lamp-wrapper canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
