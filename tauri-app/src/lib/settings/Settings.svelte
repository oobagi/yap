<script lang="ts">
  /**
   * Full settings UI for the Yap Tauri app.
   *
   * Sections: General, Transcription, Formatting, Appearance, History, Advanced
   * Loads/saves config via Tauri invoke commands.
   * Dark theme matching the overlay pill aesthetic.
   */

  import './settings.css';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onDestroy } from 'svelte';

  // ── Config Shape (matches Rust AppConfig with camelCase serde) ─────────

  interface AppConfig {
    hotkey: string;
    txProvider: string;
    txApiKey: string;
    txModel: string;
    fmtProvider: string;
    fmtApiKey: string;
    fmtModel: string;
    fmtStyle: string;
    onboardingComplete: boolean;
    dgSmartFormat: boolean;
    dgKeywords: string;
    dgLanguage: string;
    oaiLanguage: string;
    oaiPrompt: string;
    geminiTemperature: number;
    elLanguageCode: string;
    soundsEnabled: boolean;
    gradientEnabled: boolean;
    alwaysVisiblePill: boolean;
    historyEnabled: boolean;
  }

  // ── Provider Metadata ─────────────────────────────────────────────────

  const txProviders = [
    { value: 'none', label: 'None (On-device)' },
    { value: 'gemini', label: 'Gemini' },
    { value: 'openai', label: 'OpenAI' },
    { value: 'deepgram', label: 'Deepgram' },
    { value: 'elevenlabs', label: 'ElevenLabs' },
  ];

  const fmtProviders = [
    { value: 'none', label: 'None' },
    { value: 'gemini', label: 'Gemini' },
    { value: 'openai', label: 'OpenAI' },
    { value: 'anthropic', label: 'Anthropic' },
    { value: 'groq', label: 'Groq' },
  ];

  const txDefaultModels: Record<string, string> = {
    none: '',
    gemini: 'gemini-2.0-flash',
    openai: 'whisper-1',
    deepgram: 'nova-2',
    elevenlabs: 'scribe_v1',
  };

  const fmtDefaultModels: Record<string, string> = {
    none: '',
    gemini: 'gemini-2.0-flash',
    openai: 'gpt-4o-mini',
    anthropic: 'claude-sonnet-4-20250514',
    groq: 'llama-3.3-70b-versatile',
  };

  const styleData: Record<string, { label: string; description: string; example: string }> = {
    casual: {
      label: 'Casual',
      description: 'Lowercase, minimal punctuation, conversational tone',
      example: 'yeah i was thinking we could try that new place on friday if you\'re free',
    },
    formatted: {
      label: 'Formatted',
      description: 'Proper capitalization and punctuation, natural writing style',
      example: 'Yeah, I was thinking we could try that new place on Friday if you\'re free.',
    },
    professional: {
      label: 'Professional',
      description: 'Polished, clear, and business-appropriate language',
      example: 'I was considering whether we might visit the new restaurant on Friday, if your schedule allows.',
    },
  };

  const styleExampleInput = 'yeah i was thinking we could try that new place on friday if youre free';

  // ── State ─────────────────────────────────────────────────────────────

  let loading = $state(true);
  let saving = $state(false);

  // General
  let hotkey = $state('fn');
  let capturingHotkey = $state(false);
  let microphones = $state<string[]>([]);
  let selectedMic = $state('');

  // Transcription
  let txProvider = $state('none');
  let txApiKey = $state('');
  let txModel = $state('');
  let showTxApiKey = $state(false);

  // Transcription provider options
  let dgSmartFormat = $state(true);
  let dgKeywords = $state('');
  let dgLanguage = $state('');
  let oaiLanguage = $state('');
  let oaiPrompt = $state('');
  let geminiTemperature = $state(0);
  let elLanguageCode = $state('');

  // Formatting
  let fmtProvider = $state('none');
  let fmtApiKey = $state('');
  let fmtModel = $state('');
  let fmtStyle = $state('formatted');
  let fmtUseSameKey = $state(true);
  let showFmtApiKey = $state(false);

  // Appearance
  let soundsEnabled = $state(true);
  let gradientEnabled = $state(true);
  let alwaysVisiblePill = $state(true);
  let startWithSystem = $state(false);

  // History
  let historyEnabled = $state(true);

  // Advanced
  let speechLocale = $state('');
  let onboardingComplete = $state(false);

  // ── Derived ───────────────────────────────────────────────────────────

  let hasTxProvider = $derived(txProvider !== 'none');
  let hasFmtProvider = $derived(fmtProvider !== 'none');

  let canShareApiKey = $derived.by(() => {
    if (!hasTxProvider || !hasFmtProvider) return false;
    return (
      (txProvider === 'gemini' && fmtProvider === 'gemini') ||
      (txProvider === 'openai' && fmtProvider === 'openai')
    );
  });

  let effectiveFmtApiKey = $derived(
    fmtUseSameKey && canShareApiKey ? txApiKey : fmtApiKey
  );

  let currentStyleData = $derived(styleData[fmtStyle] ?? styleData.formatted);

  // ── Load Config ───────────────────────────────────────────────────────

  async function loadConfig() {
    try {
      const cfg = await invoke<AppConfig>('get_config');
      hotkey = cfg.hotkey;
      txProvider = cfg.txProvider;
      txApiKey = cfg.txApiKey;
      txModel = cfg.txModel;
      fmtProvider = cfg.fmtProvider;
      fmtApiKey = cfg.fmtApiKey;
      fmtModel = cfg.fmtModel;
      fmtStyle = cfg.fmtStyle;
      onboardingComplete = cfg.onboardingComplete;
      dgSmartFormat = cfg.dgSmartFormat;
      dgKeywords = cfg.dgKeywords;
      dgLanguage = cfg.dgLanguage;
      oaiLanguage = cfg.oaiLanguage;
      oaiPrompt = cfg.oaiPrompt;
      geminiTemperature = cfg.geminiTemperature;
      elLanguageCode = cfg.elLanguageCode;
      soundsEnabled = cfg.soundsEnabled;
      gradientEnabled = cfg.gradientEnabled;
      alwaysVisiblePill = cfg.alwaysVisiblePill;
      historyEnabled = cfg.historyEnabled;

      // Determine if formatting shares the transcription key
      fmtUseSameKey = cfg.fmtApiKey === '' || cfg.fmtApiKey === cfg.txApiKey;
    } catch (e) {
      console.error('Failed to load config:', e);
    }

    try {
      const devices = await invoke<string[]>('list_audio_devices');
      microphones = devices;
      if (devices.length > 0 && !selectedMic) {
        selectedMic = devices[0];
      }
    } catch (e) {
      console.error('Failed to list audio devices:', e);
    }

    loading = false;
  }

  // ── Save Config ───────────────────────────────────────────────────────

  async function saveConfig() {
    saving = true;
    try {
      const cfg: AppConfig = {
        hotkey,
        txProvider,
        txApiKey,
        txModel,
        fmtProvider,
        fmtApiKey: fmtUseSameKey && canShareApiKey ? '' : fmtApiKey,
        fmtModel,
        fmtStyle,
        onboardingComplete,
        dgSmartFormat,
        dgKeywords,
        dgLanguage,
        oaiLanguage,
        oaiPrompt,
        geminiTemperature,
        elLanguageCode,
        soundsEnabled,
        gradientEnabled,
        alwaysVisiblePill,
        historyEnabled,
      };

      await invoke('save_config', { cfg });
      closeWindow();
    } catch (e) {
      console.error('Failed to save config:', e);
    }
    saving = false;
  }

  // ── Close Window ──────────────────────────────────────────────────────

  function closeWindow() {
    getCurrentWindow().hide();
  }

  // ── Keyboard ──────────────────────────────────────────────────────────

  function onKeyDown(e: KeyboardEvent) {
    if (capturingHotkey) {
      e.preventDefault();
      e.stopPropagation();

      // Map key to supported hotkeys
      if (e.key === 'Alt' || e.key === 'Option') {
        hotkey = 'option';
        capturingHotkey = false;
      } else if (e.key === 'fn' || e.key === 'F24') {
        hotkey = 'fn';
        capturingHotkey = false;
      } else if (e.key === 'Escape') {
        capturingHotkey = false;
      }
      return;
    }

    if (e.key === 'Escape') {
      closeWindow();
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      saveConfig();
    }
  }

  // ── Hotkey Display ────────────────────────────────────────────────────

  function hotkeyDisplayLabel(key: string): string {
    if (key === 'fn') return 'fn / Globe';
    if (key === 'option') return 'Option';
    return key;
  }

  // ── Reset Onboarding ─────────────────────────────────────────────────

  function resetOnboarding() {
    onboardingComplete = false;
  }

  // ── Init ──────────────────────────────────────────────────────────────

  // Load config immediately on mount.
  loadConfig();

  // Re-load config whenever the settings window is shown / focused, so
  // the form always reflects the latest persisted values (the window is
  // hidden rather than destroyed when closed).
  let unlistenFocus: (() => void) | undefined;

  getCurrentWindow()
    .onFocusChanged(({ payload: focused }) => {
      if (focused) {
        loading = true;
        loadConfig();
      }
    })
    .then((fn) => {
      unlistenFocus = fn;
    });

  onDestroy(() => {
    unlistenFocus?.();
  });
</script>

<svelte:window onkeydown={onKeyDown} />

{#if loading}
  <div class="settings-container" style="align-items: center; justify-content: center;">
    <span style="color: var(--settings-text-muted); font-size: 13px;">Loading...</span>
  </div>
{:else}
  <div class="settings-container">
    <!-- Scrollable Content -->
    <div class="settings-scroll">

      <!-- ── General ──────────────────────────────────────────────────── -->
      <div class="settings-section">
        <div class="section-header">General</div>
        <div class="section-body">
          <!-- Hotkey -->
          <div class="field-row">
            <span class="field-label">Hotkey</span>
            <button
              class="hotkey-button"
              class:capturing={capturingHotkey}
              onclick={() => { capturingHotkey = !capturingHotkey; }}
            >
              {#if capturingHotkey}
                Press a key...
              {:else}
                {hotkeyDisplayLabel(hotkey)}
              {/if}
            </button>
            <span class="field-description">Press fn (Globe) or Option to set your recording hotkey.</span>
          </div>

          <div class="field-divider"></div>

          <!-- Microphone -->
          <div class="field-row">
            <span class="field-label">Microphone</span>
            <select class="select" bind:value={selectedMic}>
              {#each microphones as mic}
                <option value={mic}>{mic}</option>
              {/each}
              {#if microphones.length === 0}
                <option value="">No devices found</option>
              {/if}
            </select>
          </div>
        </div>
      </div>

      <!-- ── Transcription ────────────────────────────────────────────── -->
      <div class="settings-section">
        <div class="section-header">Transcription</div>
        <div class="section-body">
          <!-- Provider -->
          <div class="field-row">
            <span class="field-label">Provider</span>
            <select class="select" bind:value={txProvider}>
              {#each txProviders as p}
                <option value={p.value}>{p.label}</option>
              {/each}
            </select>
          </div>

          {#if hasTxProvider}
            <div class="field-divider"></div>

            <!-- API Key -->
            <div class="field-row">
              <span class="field-label">API Key</span>
              <div class="password-wrapper">
                <input
                  class="input"
                  type={showTxApiKey ? 'text' : 'password'}
                  placeholder="Required"
                  bind:value={txApiKey}
                  autocomplete="off"
                />
                <button
                  class="password-toggle"
                  onclick={() => { showTxApiKey = !showTxApiKey; }}
                  aria-label={showTxApiKey ? 'Hide API key' : 'Show API key'}
                  type="button"
                >
                  {#if showTxApiKey}
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                      <path d="M1 8s2.5-5 7-5 7 5 7 5-2.5 5-7 5-7-5-7-5Z"/>
                      <circle cx="8" cy="8" r="2"/>
                    </svg>
                  {:else}
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                      <path d="M1 8s2.5-5 7-5 7 5 7 5-2.5 5-7 5-7-5-7-5Z"/>
                      <circle cx="8" cy="8" r="2"/>
                      <line x1="2" y1="14" x2="14" y2="2"/>
                    </svg>
                  {/if}
                </button>
              </div>
            </div>

            <!-- Model -->
            <div class="field-row">
              <span class="field-label">Model</span>
              <input
                class="input"
                type="text"
                placeholder={txDefaultModels[txProvider] ?? ''}
                bind:value={txModel}
              />
              <span class="field-description">
                Leave empty to use the default ({txDefaultModels[txProvider] ?? 'none'}).
              </span>
            </div>

            <!-- Provider-specific options -->
            {#if txProvider === 'deepgram'}
              <div class="field-divider"></div>

              <div class="toggle-row">
                <div class="toggle-info">
                  <span class="toggle-label">Smart Format</span>
                  <span class="toggle-description">Auto-formats numbers, dates, currencies, and adds punctuation</span>
                </div>
                <label class="toggle-switch">
                  <input type="checkbox" bind:checked={dgSmartFormat} />
                  <span class="toggle-track"></span>
                  <span class="toggle-thumb"></span>
                </label>
              </div>

              <div class="field-row">
                <span class="field-label">Language</span>
                <input
                  class="input"
                  type="text"
                  placeholder="Auto-detect"
                  bind:value={dgLanguage}
                />
                <span class="field-description">ISO 639-1 language code (e.g. en, es, fr, ja). Leave empty to auto-detect.</span>
              </div>

              <div class="field-row">
                <span class="field-label">Keywords</span>
                <input
                  class="input"
                  type="text"
                  placeholder="e.g. Kubernetes, Jira, OAuth"
                  bind:value={dgKeywords}
                />
                <span class="field-description">Boost recognition of specific words or names, separated by commas.</span>
              </div>
            {/if}

            {#if txProvider === 'openai'}
              <div class="field-divider"></div>

              <div class="field-row">
                <span class="field-label">Language</span>
                <input
                  class="input"
                  type="text"
                  placeholder="Auto-detect"
                  bind:value={oaiLanguage}
                />
                <span class="field-description">ISO 639-1 language code (e.g. en, es, fr). Improves accuracy and speed.</span>
              </div>

              <div class="field-row">
                <span class="field-label">Prompt</span>
                <input
                  class="input"
                  type="text"
                  placeholder="e.g. The speaker discusses SwiftUI and Xcode"
                  bind:value={oaiPrompt}
                />
                <span class="field-description">Guide the model with context -- useful for domain-specific terms, names, or jargon it might mishear.</span>
              </div>
            {/if}

            {#if txProvider === 'gemini'}
              <div class="field-divider"></div>

              <div class="field-row">
                <span class="field-label">Temperature</span>
                <div class="slider-row">
                  <input
                    class="slider-input"
                    type="range"
                    min="0"
                    max="1"
                    step="0.1"
                    bind:value={geminiTemperature}
                  />
                  <span class="slider-value">{geminiTemperature.toFixed(1)}</span>
                </div>
                <span class="field-description">Controls randomness. 0 = precise and deterministic, 1 = creative and varied. Lower is better for transcription.</span>
              </div>
            {/if}

            {#if txProvider === 'elevenlabs'}
              <div class="field-divider"></div>

              <div class="field-row">
                <span class="field-label">Language Code</span>
                <input
                  class="input"
                  type="text"
                  placeholder="Auto-detect"
                  bind:value={elLanguageCode}
                />
                <span class="field-description">ISO 639-1 language code (e.g. en, es, fr). Leave empty to auto-detect.</span>
              </div>
            {/if}
          {/if}
        </div>
        {#if !hasTxProvider}
          <div class="section-footer">
            Using built-in on-device speech recognition -- free, no API key needed.
          </div>
        {/if}
      </div>

      <!-- ── Formatting ───────────────────────────────────────────────── -->
      <div class="settings-section">
        <div class="section-header">Formatting</div>
        <div class="section-body">
          <!-- Provider -->
          <div class="field-row">
            <span class="field-label">Provider</span>
            <select class="select" bind:value={fmtProvider}>
              {#each fmtProviders as p}
                <option value={p.value}>{p.label}</option>
              {/each}
            </select>
          </div>

          {#if hasFmtProvider}
            <div class="field-divider"></div>

            <!-- API Key -->
            <div class="field-row">
              <span class="field-label">API Key</span>
              <div class="password-wrapper">
                <input
                  class="input"
                  type={showFmtApiKey ? 'text' : 'password'}
                  placeholder="Required"
                  value={effectiveFmtApiKey}
                  oninput={(e: Event) => { fmtApiKey = (e.target as HTMLInputElement).value; }}
                  disabled={fmtUseSameKey && canShareApiKey}
                  autocomplete="off"
                />
                <button
                  class="password-toggle"
                  onclick={() => { showFmtApiKey = !showFmtApiKey; }}
                  aria-label={showFmtApiKey ? 'Hide API key' : 'Show API key'}
                  type="button"
                >
                  {#if showFmtApiKey}
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                      <path d="M1 8s2.5-5 7-5 7 5 7 5-2.5 5-7 5-7-5-7-5Z"/>
                      <circle cx="8" cy="8" r="2"/>
                    </svg>
                  {:else}
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                      <path d="M1 8s2.5-5 7-5 7 5 7 5-2.5 5-7 5-7-5-7-5Z"/>
                      <circle cx="8" cy="8" r="2"/>
                      <line x1="2" y1="14" x2="14" y2="2"/>
                    </svg>
                  {/if}
                </button>
              </div>
              {#if canShareApiKey}
                <label class="checkbox-row">
                  <input type="checkbox" bind:checked={fmtUseSameKey} />
                  <span class="checkbox-label">Use same API key as transcription</span>
                </label>
              {/if}
            </div>

            <!-- Model -->
            <div class="field-row">
              <span class="field-label">Model</span>
              <input
                class="input"
                type="text"
                placeholder={fmtDefaultModels[fmtProvider] ?? ''}
                bind:value={fmtModel}
              />
              <span class="field-description">
                Leave empty to use the default ({fmtDefaultModels[fmtProvider] ?? 'none'}).
              </span>
            </div>

            <div class="field-divider"></div>

            <!-- Style Picker -->
            <div class="field-row">
              <span class="field-label">Style</span>
              <div class="style-picker">
                {#each Object.entries(styleData) as [value, data]}
                  <div class="style-option">
                    <input
                      type="radio"
                      name="fmtStyle"
                      id="style-{value}"
                      {value}
                      checked={fmtStyle === value}
                      onchange={() => { fmtStyle = value; }}
                    />
                    <label for="style-{value}">{data.label}</label>
                  </div>
                {/each}
              </div>
            </div>

            <!-- Style Preview -->
            <div class="style-preview">
              <div class="style-preview-header">
                <div class="style-preview-title">{currentStyleData.label}</div>
                <div class="style-preview-desc">{currentStyleData.description}</div>
              </div>
              <div class="style-preview-body">
                <div class="style-preview-col">
                  <div class="style-preview-label before">Before</div>
                  <div class="style-preview-text">{styleExampleInput}</div>
                </div>
                <div class="style-preview-col">
                  <div class="style-preview-label after">After</div>
                  <div class="style-preview-text">{currentStyleData.example}</div>
                </div>
              </div>
            </div>
          {/if}
        </div>
        {#if !hasFmtProvider}
          <div class="section-footer">
            No formatting -- raw transcription will be pasted as-is.
          </div>
        {/if}
      </div>

      <!-- ── Appearance ───────────────────────────────────────────────── -->
      <div class="settings-section">
        <div class="section-header">Appearance</div>
        <div class="section-body">
          <div class="toggle-row">
            <div class="toggle-info">
              <span class="toggle-label">Sound effects</span>
            </div>
            <label class="toggle-switch">
              <input type="checkbox" bind:checked={soundsEnabled} />
              <span class="toggle-track"></span>
              <span class="toggle-thumb"></span>
            </label>
          </div>

          <div class="field-divider"></div>

          <div class="toggle-row">
            <div class="toggle-info">
              <span class="toggle-label">Gradient background</span>
            </div>
            <label class="toggle-switch">
              <input type="checkbox" bind:checked={gradientEnabled} />
              <span class="toggle-track"></span>
              <span class="toggle-thumb"></span>
            </label>
          </div>

          <div class="field-divider"></div>

          <div class="toggle-row">
            <div class="toggle-info">
              <span class="toggle-label">Always-visible pill</span>
              <span class="toggle-description">Keep the overlay pill visible even when idle</span>
            </div>
            <label class="toggle-switch">
              <input type="checkbox" bind:checked={alwaysVisiblePill} />
              <span class="toggle-track"></span>
              <span class="toggle-thumb"></span>
            </label>
          </div>

          <div class="field-divider"></div>

          <div class="toggle-row">
            <div class="toggle-info">
              <span class="toggle-label">Start with system</span>
              <span class="toggle-description">Launch Yap automatically when you log in</span>
            </div>
            <label class="toggle-switch">
              <input type="checkbox" bind:checked={startWithSystem} />
              <span class="toggle-track"></span>
              <span class="toggle-thumb"></span>
            </label>
          </div>
        </div>
      </div>

      <!-- ── History ──────────────────────────────────────────────────── -->
      <div class="settings-section">
        <div class="section-header">History</div>
        <div class="section-body">
          <div class="toggle-row">
            <div class="toggle-info">
              <span class="toggle-label">Save transcription history</span>
            </div>
            <label class="toggle-switch">
              <input type="checkbox" bind:checked={historyEnabled} />
              <span class="toggle-track"></span>
              <span class="toggle-thumb"></span>
            </label>
          </div>
        </div>
        {#if !historyEnabled}
          <div class="section-footer">
            Transcriptions will not be saved to disk.
          </div>
        {/if}
      </div>

      <!-- ── Advanced ─────────────────────────────────────────────────── -->
      <div class="settings-section">
        <div class="section-header">Advanced</div>
        <div class="section-body">
          <div class="field-row">
            <span class="field-label">Speech recognition locale</span>
            <input
              class="input"
              type="text"
              placeholder="en-US"
              bind:value={speechLocale}
            />
            <span class="field-description">BCP 47 locale for on-device speech recognition (e.g. en-US, ja-JP, fr-FR).</span>
          </div>

          <div class="field-divider"></div>

          <div style="display: flex; align-items: center; justify-content: space-between;">
            <div>
              <span class="field-label">Reset onboarding</span>
              <div class="field-description" style="margin-top: 2px;">Show the welcome tutorial again on next launch</div>
            </div>
            <button class="btn btn-secondary" onclick={resetOnboarding} type="button">
              Reset
            </button>
          </div>
        </div>
      </div>

    </div>

    <!-- ── Footer ─────────────────────────────────────────────────────── -->
    <div class="settings-footer">
      <div class="settings-footer-left">
        <button class="btn btn-danger-ghost" onclick={resetOnboarding} type="button">
          Reset Onboarding
        </button>
      </div>
      <button class="btn btn-secondary" onclick={closeWindow} type="button">
        Cancel
      </button>
      <button class="btn btn-primary" onclick={saveConfig} disabled={saving} type="button">
        {#if saving}
          Saving...
        {:else}
          Save
        {/if}
      </button>
    </div>
  </div>
{/if}
