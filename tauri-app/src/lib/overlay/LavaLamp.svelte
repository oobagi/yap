<script lang="ts">
  /**
   * Lava lamp gradient using CSS radial-gradients.
   * Blobs move as a group (shared drift) with small individual offsets.
   * Energy controls intensity via CSS opacity (smooth transition).
   * Blob positions never depend on energy — prevents jumping.
   */

  let { energy = 0.5, visible = true }: { energy?: number; visible?: boolean } = $props();

  let t = $state(0);
  let animId = 0;
  let startTime = 0;

  const blobs = [
    { color: '147, 51, 234',  offsetX: -8,  offsetY: -3,  size: 220, scale: 1.0  },  // purple
    { color: '59, 130, 246',  offsetX:  5,  offsetY:  2,  size: 260, scale: 0.9  },  // blue
    { color: '34, 211, 238',  offsetX: -4,  offsetY:  4,  size: 190, scale: 0.85 },  // cyan
    { color: '99, 102, 241',  offsetX:  7,  offsetY: -2,  size: 240, scale: 0.9  },  // indigo
  ];

  // Blob positions — NO dependency on energy, only on t
  let blobStyles = $derived.by(() => {
    const groupX = Math.cos(t * 0.2) * 6;
    const groupY = Math.sin(t * 0.15) * 3;

    return blobs.map((b, i) => {
      const wiggleX = Math.sin(t * (0.35 + i * 0.1) + i * 1.5) * 3;
      const wiggleY = Math.cos(t * (0.25 + i * 0.08) + i * 2.0) * 2;

      const x = 50 + groupX + b.offsetX + wiggleX;
      const y = 90 + groupY + b.offsetY + wiggleY;

      const alpha = 0.7 * b.scale;
      return `radial-gradient(ellipse ${b.size}px ${b.size * 0.55}px at ${x}% ${y}%, rgba(${b.color}, ${alpha.toFixed(2)}) 0%, rgba(${b.color}, 0) 70%)`;
    });
  });

  // Energy only controls opacity — smoothly transitions via CSS
  let intensityOpacity = $derived(0.4 + energy * 0.6);

  function loop(timestamp: number) {
    if (!startTime) startTime = timestamp;
    t = (timestamp - startTime) / 1000;
    animId = requestAnimationFrame(loop);
  }

  $effect(() => {
    if (visible) {
      startTime = 0;
      animId = requestAnimationFrame(loop);
    } else {
      if (animId) cancelAnimationFrame(animId);
      animId = 0;
    }
    return () => {
      if (animId) cancelAnimationFrame(animId);
    };
  });
</script>

<div class="lava-lamp-outer" class:active={visible}>
  <div
    class="lava-lamp-inner"
    style="background: {blobStyles.join(', ')};"
  ></div>
</div>

<style>
  .lava-lamp-outer {
    position: absolute;
    inset: -40px;
    pointer-events: none;
    opacity: 0;
    transform: translateY(30px);
    transition: opacity 600ms cubic-bezier(0.16, 1, 0.3, 1),
                transform 600ms cubic-bezier(0.16, 1, 0.3, 1);
  }

  .lava-lamp-outer.active {
    opacity: 1;
    transform: translateY(0);
  }

  .lava-lamp-inner {
    width: 100%;
    height: 100%;
    filter: blur(40px);
  }
</style>
