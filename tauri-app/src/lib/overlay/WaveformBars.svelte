<script lang="ts">
  /**
   * Waveform bar visualizer with 11 bars.
   * During recording: bars react to audio levels.
   * During processing: Gaussian shimmer wave sweeps across bars.
   * Uses position scaling so center bars are taller than edge bars.
   */

  let {
    bandLevels = Array(11).fill(0),
    mode = 'idle',
    isPaused = false,
  }: {
    bandLevels?: number[];
    mode?: string;
    isPaused?: boolean;
  } = $props();

  const BAR_COUNT = 11;
  const MIN_HEIGHT = 5;
  const MAX_HEIGHT = 28;

  // Position scale — center emphasis, edges still move
  const positionScale = [0.35, 0.45, 0.6, 0.78, 0.92, 1.0, 0.94, 0.8, 0.63, 0.48, 0.38];

  // Processing shimmer state
  let shimmerFrame: number = 0;
  let shimmerStart: number = 0;
  let shimmerHeights: number[] = $state(Array(BAR_COUNT).fill(MIN_HEIGHT));
  let shimmerOpacities: number[] = $state(Array(BAR_COUNT).fill(0.9));
  let audioDecay: number = $state(1);
  let waveStrength: number = $state(0);
  let appeared: boolean = $state(false);

  // Computed bar heights based on mode
  let barHeights: number[] = $derived.by(() => {
    if (isPaused) {
      return Array(BAR_COUNT).fill(MIN_HEIGHT);
    }

    if (mode === 'processing') {
      return shimmerHeights;
    }

    if (mode === 'noSpeech') {
      return Array(BAR_COUNT).fill(MIN_HEIGHT);
    }

    // Recording or idle with onboarding — audio-reactive bars
    return computeAudioBars(bandLevels);
  });

  let barOpacities: number[] = $derived.by(() => {
    if (isPaused || mode === 'noSpeech') {
      return Array(BAR_COUNT).fill(0.25);
    }
    if (mode === 'processing') {
      return shimmerOpacities;
    }
    return Array(BAR_COUNT).fill(0.9);
  });

  function computeAudioBars(levels: number[]): number[] {
    const heights: number[] = [];
    const overall = levels.length > 0
      ? levels.reduce((a, b) => a + b, 0) / levels.length
      : 0;

    for (let i = 0; i < BAR_COUNT; i++) {
      const scale = positionScale[i];
      const bandLevel = i < levels.length ? levels[i] : 0;
      const blended = (overall * 0.7 + bandLevel * 0.3) * audioDecay;
      const barCeiling = MIN_HEIGHT + (MAX_HEIGHT - MIN_HEIGHT) * scale;
      const scaled = Math.min(blended / 0.75, 1.0);
      const driven = Math.pow(scaled, 0.6);
      const audioH = Math.max(MIN_HEIGHT, Math.min(barCeiling, MIN_HEIGHT + (barCeiling - MIN_HEIGHT) * driven));
      heights.push(audioH);
    }
    return heights;
  }

  function shimmerLoop(timestamp: number) {
    if (mode !== 'processing') return;

    if (!shimmerStart) shimmerStart = timestamp;
    const elapsed = (timestamp - shimmerStart) / 1000;

    const margin = 5.0;
    const sweepRange = (BAR_COUNT - 1) + margin * 2;
    const t = (elapsed % 1.2) / 1.2;
    const waveCenter = -margin + t * sweepRange;

    const newHeights: number[] = [];
    const newOpacities: number[] = [];

    for (let i = 0; i < BAR_COUNT; i++) {
      const scale = positionScale[i];
      const bandLevel = i < bandLevels.length ? bandLevels[i] : 0;
      const overall = bandLevels.length > 0
        ? bandLevels.reduce((a, b) => a + b, 0) / bandLevels.length
        : 0;

      const blended = (overall * 0.7 + bandLevel * 0.3) * audioDecay;
      const barCeiling = MIN_HEIGHT + (MAX_HEIGHT - MIN_HEIGHT) * scale;
      const scaled = Math.min(blended / 0.75, 1.0);
      const driven = Math.pow(scaled, 0.6);
      const audioH = Math.max(MIN_HEIGHT, Math.min(barCeiling, MIN_HEIGHT + (barCeiling - MIN_HEIGHT) * driven));

      // Gaussian wave overlay
      const distance = Math.abs(i - waveCenter);
      const wave = Math.exp(-distance * distance / 6.0);
      const waveH = 14.0 * wave * waveStrength;

      newHeights.push(Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, audioH + waveH)));

      // Shimmer opacity — bright at wave peak, dim at edges
      const dimOpacity = 0.35;
      const brightOpacity = 0.95;
      newOpacities.push(dimOpacity + (brightOpacity - dimOpacity) * wave * waveStrength);
    }

    shimmerHeights = newHeights;
    shimmerOpacities = newOpacities;
    shimmerFrame = requestAnimationFrame(shimmerLoop);
  }

  // Transition to/from processing mode
  let prevMode: string = $state('idle');

  $effect(() => {
    if (mode === 'processing' && prevMode !== 'processing') {
      // Entering processing — decay audio, ramp up wave
      shimmerStart = 0;
      transitionToProcessing();
      shimmerFrame = requestAnimationFrame(shimmerLoop);
    } else if (mode !== 'processing' && prevMode === 'processing') {
      // Leaving processing
      cancelAnimationFrame(shimmerFrame);
      shimmerFrame = 0;
      audioDecay = 1;
      waveStrength = 0;
    }
    prevMode = mode;
  });

  function transitionToProcessing() {
    // Animate audioDecay from current to 0 over 350ms
    const decayStart = performance.now();
    const decayFrom = audioDecay;

    function decayStep(ts: number) {
      const progress = Math.min((ts - decayStart) / 350, 1);
      // Ease out
      audioDecay = decayFrom * (1 - easeOut(progress));
      if (progress < 1) requestAnimationFrame(decayStep);
    }
    requestAnimationFrame(decayStep);

    // Animate waveStrength from 0 to 1 over 350ms, delayed 150ms
    setTimeout(() => {
      const waveStart = performance.now();

      function waveStep(ts: number) {
        const progress = Math.min((ts - waveStart) / 350, 1);
        waveStrength = easeIn(progress);
        if (progress < 1) requestAnimationFrame(waveStep);
      }
      requestAnimationFrame(waveStep);
    }, 150);
  }

  function easeOut(t: number): number {
    return 1 - Math.pow(1 - t, 3);
  }

  function easeIn(t: number): number {
    return t * t * t;
  }

  // Bar entrance animation
  $effect(() => {
    if (mode === 'recording' || mode === 'processing') {
      setTimeout(() => {
        appeared = true;
      }, 50);
    } else if (mode === 'idle') {
      appeared = false;
    }

    return () => {
      if (shimmerFrame) {
        cancelAnimationFrame(shimmerFrame);
        shimmerFrame = 0;
      }
    };
  });
</script>

<div
  class="bars-container"
  class:animate-bars-enter={appeared}
  style="
    transform: scaleX({appeared ? 1 : 0.001});
    opacity: {appeared ? 1 : 0};
    transition: transform 300ms cubic-bezier(0.16, 1, 0.3, 1), opacity 300ms cubic-bezier(0.16, 1, 0.3, 1);
  "
>
  {#each barHeights as height, i}
    <div
      class="bar"
      class:paused={isPaused}
      style="
        height: {height}px;
        opacity: {barOpacities[i]};
      "
    ></div>
  {/each}
</div>

<style>
  .bars-container {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
    width: 52px;
    height: 28px;
    transform-origin: center;
  }

  .bar {
    width: 3px;
    min-height: 5px;
    border-radius: 1.5px;
    background: white;
    transition: height 60ms cubic-bezier(0.16, 1, 0.3, 1);
    will-change: height, opacity;
  }

  .bar.paused {
    opacity: 0.25 !important;
    height: 5px !important;
    transition: height 200ms ease, opacity 200ms ease;
  }
</style>
