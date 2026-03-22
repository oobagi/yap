import { mount } from 'svelte';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import Overlay from './lib/overlay/Overlay.svelte';

// Reactive state shared with the Svelte component via object reference
const overlayState = $state({
  mode: 'idle' as 'idle' | 'recording' | 'processing' | 'noSpeech' | 'error',
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
});

// Mount the Overlay component
const app = mount(Overlay, {
  target: document.getElementById('overlay')!,
  props: { overlayData: overlayState },
});

// ── Tauri IPC Event Listeners ──────────────────────────────────────────

listen<{ level: number; bars: number[] }>('audio:levels', (event) => {
  overlayState.bandLevels = event.payload.bars;
  overlayState.audioLevel = event.payload.level;
});

listen<{ state: string; handsFree?: boolean; paused?: boolean; elapsed?: number }>(
  'state:change',
  (event) => {
    const { state, handsFree, paused, elapsed } = event.payload;

    if (state === 'recording' || state === 'processing' || state === 'idle' || state === 'noSpeech') {
      overlayState.mode = state;
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
    overlayState.onboardingStep = event.payload.step;
    overlayState.onboardingText = event.payload.text;
    if (event.payload.hotkeyLabel) {
      overlayState.hotkeyLabel = event.payload.hotkeyLabel;
    }
  }
);

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

// ── Click-Through Management ───────────────────────────────────────────

const appWindow = getCurrentWindow();

// Start with click-through enabled so the overlay doesn't block interaction
appWindow.setIgnoreCursorEvents(true);

// The pill element dispatches custom events to toggle click-through
window.addEventListener('pill:mouseenter', () => {
  appWindow.setIgnoreCursorEvents(false);
});

window.addEventListener('pill:mouseleave', () => {
  appWindow.setIgnoreCursorEvents(true);
});
