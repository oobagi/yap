<script lang="ts">
  /**
   * Main overlay component — the floating glass pill with all visual states.
   *
   * States:
   *   idle        — minimized pill
   *   recording   — expanded pill with audio-reactive waveform bars
   *   processing  — slightly contracted (scale 0.8) with shimmer sweep
   *   noSpeech    — flat bars + shake animation
   *   error       — warning icon + message, auto-dismisses
   *
   * Also handles:
   *   - Onboarding cards above the pill
   *   - Hands-free pause/stop buttons
   *   - Hover tooltip in idle state
   *   - Elapsed timer during recording
   *   - Lava lamp gradient background
   */

  import './overlay.css';
  import WaveformBars from './WaveformBars.svelte';
  import LavaLamp from './LavaLamp.svelte';
  import Prompt from './Prompt.svelte';

  interface OverlayData {
    mode: 'idle' | 'pending' | 'recording' | 'processing' | 'noSpeech' | 'error';
    bandLevels: number[];
    audioLevel: number;
    errorMessage: string;
    onboardingText: string;
    onboardingStep: string | null;
    isHandsFree: boolean;
    isPaused: boolean;
    gradientEnabled: boolean;
    alwaysVisible: boolean;
    handsFreeElapsed: number;
    hotkeyLabel: string;
    visible: boolean;
    celebrating: boolean;
    onboardingPressed: boolean;
  }

  let { overlayData }: { overlayData: OverlayData } = $props();

  // ── Local State ────────────────────────────────────────────────────────

  let isPressed = $state(false);
  let shaking = $state(false);
  let slideVisible = $state<'in' | 'out' | 'none'>('none');
  let prevMode = $state('idle');

  // ── Derived Values ─────────────────────────────────────────────────────

  let isExpanded = $derived(overlayData.mode !== 'idle' || overlayData.onboardingStep !== null);
  let isMinimized = $derived(overlayData.mode === 'idle' && overlayData.onboardingStep === null);
  let showTimer = $derived(
    overlayData.mode === 'recording' && overlayData.handsFreeElapsed >= 10
  );

  let elapsedFormatted = $derived.by(() => {
    const s = Math.floor(overlayData.handsFreeElapsed);
    const mins = Math.floor(s / 60);
    const secs = s % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  });

  // Gradient energy level based on mode
  let gradientEnergy = $derived.by(() => {
    switch (overlayData.mode) {
      case 'recording': return 1.0;
      case 'processing': return 0.6;
      case 'pending': return 0.0;
      default:
        return overlayData.onboardingStep ? 0.3 : 0.4;
    }
  });

  // Gradient only shows during active states, not pending taps.
  let showGradient = $derived(
    overlayData.gradientEnabled && isExpanded && overlayData.mode !== 'pending'
  );

  // Audio bounce — pill scales with audio level during recording
  let audioBounceFactor = $derived.by(() => {
    if (overlayData.mode !== 'recording' || overlayData.isPaused) return 1.0;
    const level = Math.min(overlayData.audioLevel, 1.0);
    return 1.0 + Math.pow(level, 1.5) * 0.12;
  });

  // Pill scale factor
  let pillScale = $derived.by(() => {
    if (isExpanded) return 0.64;
    return 0.58;
  });

  // Vertical position for the full card/timer/pill stack.
  let stackOffsetY = $derived(isExpanded ? 12 : 15);

  // Combined transform for the pill
  let pillTransform = $derived.by(() => {
    const processingScale = overlayData.mode === 'processing' ? 0.85 : 1.0;
    const pressScale = isPressed ? 0.85 : 1.0;
    const confirmScale = confirmPressed ? 0.85 : 1.0;
    const scale = pillScale * audioBounceFactor * pressScale * processingScale * confirmScale;
    return `scale(${scale})`;
  });

  // Show the "Hold fn" prompt inside the pill for specific onboarding steps
  let showHoldPrompt = $derived.by(() => {
    if (!overlayData.onboardingStep) return false;
    if (overlayData.mode !== 'idle' && overlayData.mode !== 'noSpeech') return false;
    return ['apiTip', 'formattingTip', 'welcome'].includes(overlayData.onboardingStep);
  });

  // Whether the pill is in hold-to-confirm pressed state (scale-down feedback)
  let confirmPressed = $derived(overlayData.onboardingPressed);

  // ── Shake Animation ────────────────────────────────────────────────────

  let shakeOffset = $state(0);
  let shakeAnimFrame = 0;

  function triggerShake() {
    shaking = true;
    const startTs = performance.now();
    const duration = 500;

    function shakeStep(ts: number) {
      const progress = Math.min((ts - startTs) / duration, 1);
      shakeOffset = 4 * Math.sin(progress * Math.PI * 6) * (1 - progress);
      if (progress < 1) {
        shakeAnimFrame = requestAnimationFrame(shakeStep);
      } else {
        shakeOffset = 0;
        shaking = false;
      }
    }
    shakeAnimFrame = requestAnimationFrame(shakeStep);
  }

  // ── Mode Change Effects ────────────────────────────────────────────────

  $effect(() => {
    const currentMode = overlayData.mode;

    if (currentMode === 'noSpeech' && prevMode !== 'noSpeech') {
      triggerShake();
    }

    // Slide in when becoming active
    if (currentMode !== 'idle' && prevMode === 'idle') {
      slideVisible = 'in';
    }

    // Slide out when going idle (unless always visible)
    if (currentMode === 'idle' && prevMode !== 'idle' && !overlayData.alwaysVisible && !overlayData.onboardingStep) {
      slideVisible = 'out';
    }

    prevMode = currentMode;

    return () => {
      if (shakeAnimFrame) cancelAnimationFrame(shakeAnimFrame);
    };
  });

  // ── Error Auto-Dismiss ─────────────────────────────────────────────────

  let errorTimeout: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (overlayData.mode === 'error') {
      clearTimeout(errorTimeout);
      errorTimeout = setTimeout(() => {
        overlayData.mode = 'idle';
      }, 2500);
    }

    return () => {
      clearTimeout(errorTimeout);
    };
  });

  // ── Event Handlers ─────────────────────────────────────────────────────

  function onPillClick() {
    // Don't send pill clicks during processing or hands-free (only
    // the dedicated stop/pause buttons control hands-free sessions).
    if (overlayData.mode !== 'processing' && !overlayData.isHandsFree) {
      import('@tauri-apps/api/core').then(({ invoke }) => {
        invoke('pill_clicked');
      });
    }
  }

  function onPillPointerDown() {
    isPressed = true;
  }

  function onPillPointerUp() {
    isPressed = false;
  }

  function onPauseClick(e: MouseEvent) {
    e.stopPropagation();
    import('@tauri-apps/api/core').then(({ invoke }) => {
      invoke('pause_resume');
    });
  }

  function onStopClick(e: MouseEvent) {
    e.stopPropagation();
    import('@tauri-apps/api/core').then(({ invoke }) => {
      invoke('stop_hands_free');
    });
  }
</script>

<div class="overlay-container">
  <!-- Lava lamp gradient background (around the pill area) -->
  <LavaLamp
    energy={gradientEnergy}
    visible={showGradient}
    celebrating={overlayData.celebrating}
  />

  <!-- Vertical stack: onboarding card, timer, pill -->
  <div
    class="pill-wrapper"
    class:animate-slide-in={slideVisible === 'in'}
    class:animate-slide-out={slideVisible === 'out'}
    style="--stack-y: {stackOffsetY}px;"
  >
    <!-- Onboarding card (above pill) -->
    {#if overlayData.onboardingText && overlayData.onboardingStep && (overlayData.mode === 'idle' || overlayData.mode === 'noSpeech')}
      {#key overlayData.onboardingStep}
        <Prompt
          variant="card"
          step={overlayData.onboardingStep}
          text={overlayData.onboardingText}
          hotkeyLabel={overlayData.hotkeyLabel}
        />
      {/key}
    {/if}

    <!-- Error card (above pill, same style as onboarding) -->
    {#if overlayData.mode === 'error' && overlayData.errorMessage}
      <div class="onboarding-card error-card animate-card-enter">
        <span class="error-icon">&#9888;</span> {overlayData.errorMessage}
      </div>
    {/if}

    <!-- Elapsed timer -->
    {#if showTimer}
      <div class="elapsed-timer animate-fade-slide-in">
        {elapsedFormatted}
      </div>
    {/if}

    <!-- The Pill -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="pill"
      class:idle={overlayData.mode === 'idle' && !overlayData.onboardingStep}
      class:recording={overlayData.mode === 'recording'}
      class:pending={overlayData.mode === 'pending'}
      class:processing={overlayData.mode === 'processing'}
      class:error={overlayData.mode === 'error'}
      class:expanded={isExpanded}
      class:minimized={isMinimized}
      class:hands-free={overlayData.isHandsFree}
      style="
        transform: {pillTransform} translateX({shakeOffset}px);
        opacity: {isPressed || confirmPressed ? 0.7 : 1};
      "
      role="status"
      aria-label="Yap overlay"
      onclick={onPillClick}
      onpointerdown={onPillPointerDown}
      onpointerup={onPillPointerUp}
    >
      <!-- Glass background layer -->
      <div class="pill-glass"></div>

      <!-- Pill Content -->
      {#if showHoldPrompt}
        <!-- Hold [key] prompt during certain onboarding steps -->
        <Prompt
          variant="inline-hold"
          step={overlayData.onboardingStep}
          hotkeyLabel={overlayData.hotkeyLabel}
        />
      {:else if overlayData.mode === 'error'}
        <!-- Error state — show flat bars in pill, message is in the card above -->
        <WaveformBars
          bandLevels={Array(11).fill(0)}
          mode="noSpeech"
          isPaused={false}
        />
      {:else if overlayData.mode === 'pending' || overlayData.mode === 'recording' || overlayData.mode === 'processing'}
        <!-- Pending / Recording / Processing state -->
        {#if overlayData.isHandsFree && overlayData.mode !== 'pending'}
          <div class="hands-free-controls">
            <!-- Pause / Resume button -->
            <button
              class="btn-action btn-pause"
              onclick={onPauseClick}
              aria-label={overlayData.isPaused ? 'Resume recording' : 'Pause recording'}
            >
              {#if overlayData.isPaused}
                <!-- Play icon -->
                <svg width="12" height="14" viewBox="0 0 12 14" fill="none">
                  <path d="M1 1.5L11 7L1 12.5V1.5Z" fill="currentColor"/>
                </svg>
              {:else}
                <!-- Pause icon -->
                <svg width="10" height="12" viewBox="0 0 10 12" fill="none">
                  <rect x="0" y="0" width="3.5" height="12" rx="1" fill="currentColor"/>
                  <rect x="6.5" y="0" width="3.5" height="12" rx="1" fill="currentColor"/>
                </svg>
              {/if}
            </button>

            <!-- Waveform bars -->
            <WaveformBars
              bandLevels={overlayData.bandLevels}
              mode={overlayData.mode}
              isPaused={overlayData.isPaused}
            />

            <!-- Stop button -->
            <button
              class="btn-action btn-stop"
              onclick={onStopClick}
              aria-label="Stop recording"
            >
              <!-- Stop icon -->
              <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                <rect x="0" y="0" width="10" height="10" rx="1.5" fill="currentColor"/>
              </svg>
            </button>
          </div>
        {:else}
          <!-- Standard waveform bars (no hands-free buttons) -->
          <WaveformBars
            bandLevels={overlayData.mode === 'pending' ? Array(11).fill(0) : overlayData.bandLevels}
            mode={overlayData.mode}
            isPaused={overlayData.isPaused}
          />
        {/if}
      {:else if overlayData.mode === 'noSpeech'}
        <!-- No speech detected — flat bars -->
        <WaveformBars
          bandLevels={Array(11).fill(0)}
          mode="noSpeech"
          isPaused={false}
        />
      {:else if overlayData.mode === 'idle' && overlayData.onboardingStep}
        <!-- Idle with onboarding — show flat bars -->
        <WaveformBars
          bandLevels={Array(11).fill(0)}
          mode="idle"
          isPaused={true}
        />
      {/if}
    </div>
  </div>
</div>
