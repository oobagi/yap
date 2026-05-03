import { mount } from 'svelte';
import { listen } from '@tauri-apps/api/event';
import Overlay from './lib/overlay/Overlay.svelte';

// Reactive state shared with the Svelte component via object reference
const overlayState = $state({
  mode: 'idle' as 'idle' | 'pending' | 'recording' | 'processing' | 'noSpeech' | 'error',
  bandLevels: Array(11).fill(0) as number[],
  audioLevel: 0,
  errorMessage: '',
  onboardingText: '',
  onboardingStep: null as string | null,
  isHandsFree: false,
  isPaused: false,
  gradientEnabled: true,
  alwaysVisible: true,
  handsFreeElapsed: 0,
  hotkeyLabel: 'fn',
  visible: true,
  celebrating: false,
  onboardingPressed: false,
});

// Mount the Overlay component
const app = mount(Overlay, {
  target: document.getElementById('overlay')!,
  props: { overlayData: overlayState },
});

// ── Tauri IPC Event Listeners ──────────────────────────────────────────

listen<{ level: number; bars: number[] }>('audio:levels', (event) => {
  if (overlayState.mode !== 'recording' && overlayState.mode !== 'processing') {
    overlayState.bandLevels = Array(11).fill(0);
    overlayState.audioLevel = 0;
    return;
  }
  overlayState.bandLevels = event.payload.bars;
  overlayState.audioLevel = event.payload.level;
});

listen<{ state: string; handsFree?: boolean; paused?: boolean; elapsed?: number }>(
  'state:change',
  (event) => {
    const { state, handsFree, paused, elapsed } = event.payload;

    if (state === 'recording' || state === 'processing' || state === 'idle' || state === 'pending' || state === 'noSpeech') {
      overlayState.mode = state;
      if (state === 'idle' || state === 'pending' || state === 'noSpeech') {
        overlayState.bandLevels = Array(11).fill(0);
        overlayState.audioLevel = 0;
      }
    }

    if (handsFree !== undefined) {
      overlayState.isHandsFree = handsFree;
    }
    if (paused !== undefined) {
      overlayState.isPaused = paused;
    }
    if (elapsed !== undefined) {
      overlayState.handsFreeElapsed = elapsed;
    }
  }
);

listen<{ step: string; text: string; hotkeyLabel?: string }>(
  'onboarding:step',
  (event) => {
    const prevStep = overlayState.onboardingStep;
    overlayState.onboardingStep = event.payload.step || null;
    overlayState.onboardingText = event.payload.text;
    if (event.payload.hotkeyLabel) {
      overlayState.hotkeyLabel = event.payload.hotkeyLabel;
    }
    // Trigger celebration animation when entering 'nice' step
    if (event.payload.step === 'nice' && prevStep !== 'nice') {
      overlayState.celebrating = true;
      setTimeout(() => {
        overlayState.celebrating = false;
      }, 3000);
    }
  }
);

listen<boolean>('onboarding:press', (event) => {
  overlayState.onboardingPressed = event.payload;
});

listen<{ message: string }>('error:show', (event) => {
  overlayState.mode = 'error';
  overlayState.errorMessage = event.payload.message;
});

listen<{ enabled: boolean }>('gradient:toggle', (event) => {
  overlayState.gradientEnabled = event.payload.enabled;
});

listen<{ visible: boolean }>('overlay:visibility', (event) => {
  overlayState.visible = event.payload.visible;
  overlayState.alwaysVisible = event.payload.visible;
});

// Signal the backend that all event listeners are registered and the
// overlay is ready to receive events (e.g. onboarding steps).
import { emit } from '@tauri-apps/api/event';
emit('overlay:ready');
