<script lang="ts">
  /**
   * Main overlay component — the floating glass pill with all visual states.
   *
   * States:
   *   idle        — minimized pill (scale 0.5, offset-y 40px)
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
   *   - Click-through toggle via custom events
   */

  import './overlay.css';
  import WaveformBars from './WaveformBars.svelte';
  import LavaLamp from './LavaLamp.svelte';

  interface OverlayData {
    mode: 'idle' | 'recording' | 'processing' | 'noSpeech' | 'error';
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
  }

  let { overlayData }: { overlayData: OverlayData } = $props();

  // ── Local State ────────────────────────────────────────────────────────

  let hovering = $state(false);
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
      default:
        if (hovering) return 0.15;
        return overlayData.onboardingStep ? 0.3 : 0.4;
    }
  });

  let showGradient = $derived(
    (isExpanded || hovering) && overlayData.gradientEnabled
  );

  // Audio bounce — pill scales with audio level during recording
  let audioBounceFactor = $derived.by(() => {
    if (overlayData.mode !== 'recording' || overlayData.isPaused) return 1.0;
    const level = Math.min(overlayData.audioLevel, 1.0);
    return 1.0 + Math.pow(level, 1.5) * 0.25;
  });

  // Pill scale factor
  let pillScale = $derived.by(() => {
    if (isExpanded) return 1.0;
    return hovering ? 0.65 : 0.5;
  });

  // Combined transform for the pill
  let pillTransform = $derived.by(() => {
    const scale = pillScale * audioBounceFactor * (isPressed ? 0.85 : 1.0) * (overlayData.mode === 'processing' ? 0.8 : 1.0);
    const offsetY = isExpanded ? 0 : 40;
    return `scale(${scale}) translateY(${offsetY}px)`;
  });

  // Show the "Hold fn" prompt inside the pill for specific onboarding steps
  let showHoldPrompt = $derived.by(() => {
    if (!overlayData.onboardingStep) return false;
    if (overlayData.mode !== 'idle' && overlayData.mode !== 'noSpeech') return false;
    return ['apiTip', 'formattingTip', 'welcome'].includes(overlayData.onboardingStep);
  });

  let holdPromptText = $derived(
    overlayData.onboardingStep === 'welcome' ? 'to finish' : 'to continue'
  );

  // ── Shake Animation ────────────────────────────────────────────────────

  let shakeOffset = $state(0);
  let shakeAnimFrame = 0;

  function triggerShake() {
    shaking = true;
    const startTs = performance.now();
    const duration = 500;

    function shakeStep(ts: number) {
      const progress = Math.min((ts - startTs) / duration, 1);
      shakeOffset = 10 * Math.sin(progress * Math.PI * 6) * (1 - progress);
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

    if (currentMode !== 'idle') {
      hovering = false;
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
        // The backend handles actual dismissal via state:change event
        // This just provides a visual fallback
      }, 2000);
    }

    return () => {
      clearTimeout(errorTimeout);
    };
  });

  // ── Click-Through Dispatch ─────────────────────────────────────────────

  function onPillMouseEnter() {
    window.dispatchEvent(new CustomEvent('pill:mouseenter'));
    if (isMinimized) {
      hovering = true;
    }
  }

  function onPillMouseLeave() {
    window.dispatchEvent(new CustomEvent('pill:mouseleave'));
    hovering = false;
  }

  function onPillClick() {
    if (overlayData.mode !== 'recording' && overlayData.mode !== 'processing') {
      // Emit click-to-record event back to Tauri
      import('@tauri-apps/api/event').then(({ emit }) => {
        emit('overlay:click-to-record');
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
    import('@tauri-apps/api/event').then(({ emit }) => {
      emit('overlay:pause-resume');
    });
  }

  function onStopClick(e: MouseEvent) {
    e.stopPropagation();
    import('@tauri-apps/api/event').then(({ emit }) => {
      emit('overlay:stop');
    });
  }
</script>

<div class="overlay-container">
  <!-- Lava lamp gradient background -->
  <LavaLamp
    energy={gradientEnergy}
    visible={showGradient}
  />

  <!-- Vertical stack: onboarding card, timer, pill -->
  <div
    class="pill-wrapper"
    class:animate-slide-in={slideVisible === 'in'}
    class:animate-slide-out={slideVisible === 'out'}
  >
    <!-- Onboarding card (above pill) -->
    {#if overlayData.onboardingText && overlayData.onboardingStep && (overlayData.mode === 'idle' || overlayData.mode === 'noSpeech')}
      <div class="onboarding-card animate-card-enter">
        {@html overlayData.onboardingText}
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
      class:processing={overlayData.mode === 'processing'}
      class:error={overlayData.mode === 'error'}
      class:expanded={isExpanded}
      class:minimized={isMinimized}
      class:hands-free={overlayData.isHandsFree}
      style="
        transform: {pillTransform} translateX({shakeOffset}px);
        opacity: {isPressed ? 0.7 : 1};
      "
      role="status"
      aria-label="Yap overlay"
      onmouseenter={onPillMouseEnter}
      onmouseleave={onPillMouseLeave}
      onclick={onPillClick}
      onpointerdown={onPillPointerDown}
      onpointerup={onPillPointerUp}
    >
      <!-- Glass background layer -->
      <div class="pill-glass"></div>

      <!-- Pill Content -->
      {#if showHoldPrompt}
        <!-- Hold [key] prompt during certain onboarding steps -->
        <div class="hold-prompt">
          <span>Hold</span>
          <span class="keycap">{overlayData.hotkeyLabel}</span>
          <span>{holdPromptText}</span>
        </div>
      {:else if overlayData.mode === 'error'}
        <!-- Error state -->
        <div class="error-content">
          <span class="error-icon">&#9888;</span>
          <span class="error-message">{overlayData.errorMessage}</span>
        </div>
      {:else if overlayData.mode === 'recording' || overlayData.mode === 'processing'}
        <!-- Recording / Processing state -->
        {#if overlayData.isHandsFree}
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
            bandLevels={overlayData.bandLevels}
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
      {:else if overlayData.mode === 'idle' && hovering}
        <!-- Idle + hovering — show mic icon -->
        <div class="hover-mic">
          <svg viewBox="0 0 16 22" fill="none" xmlns="http://www.w3.org/2000/svg">
            <rect x="4" y="1" width="8" height="13" rx="4" fill="rgba(255,255,255,0.9)"/>
            <path d="M1 10C1 14.4183 4.13401 18 8 18C11.866 18 15 14.4183 15 10" stroke="rgba(255,255,255,0.9)" stroke-width="1.5" stroke-linecap="round"/>
            <line x1="8" y1="18" x2="8" y2="21" stroke="rgba(255,255,255,0.9)" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
        </div>
      {/if}

      <!-- Hover tooltip (above pill when idle) -->
      {#if isMinimized && hovering}
        <div
          class="tooltip"
          style="
            animation: cardEnter 350ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
          "
        >
          Click to start transcribing
        </div>
      {/if}
    </div>
  </div>
</div>
