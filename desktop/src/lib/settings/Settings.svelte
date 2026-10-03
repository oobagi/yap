<script module lang="ts">
  declare const __APP_VERSION__: string;
  declare const __GIT_COMMIT_SHORT__: string;
  declare const __GITHUB_COMMIT_URL__: string;
</script>

<script lang="ts">
  /**
   * Full settings UI for the Yap desktop app.
   *
   * Sections: General, Transcription, Formatting, History, Advanced
   * Loads/saves config through the active desktop runtime bridge.
   * Dark theme matching the overlay pill aesthetic.
   */

  import './settings.css';
  import { onDestroy } from 'svelte';
  import CustomInstructions from './CustomInstructions.svelte';
  import {
    checkForRuntimeUpdate,
    confirmRuntime,
    hideWindow,
    invokeRuntime,
    invokeRuntimeOptional,
    isAutostartEnabled,
    isNativeRuntime,
    listenRuntimeEvent,
    onRuntimeFocusChanged,
    openExternal,
    setAutostartEnabled,
    type RuntimeDownloadEvent,
    type RuntimeUpdate,
  } from '../runtime/yap';
  import {
    appearanceModes,
    backgroundAudioModes,
    fmtDefaultModels,
    formattingProviderRequiresApiKey,
    formattingProviders,
    languageOptions,
    modifierOrder,
    providerLabels,
    settingsSections,
    styleData,
    styleExampleInput,
    transcriptionProviders,
    transcriptionProviderRequiresApiKey,
    txDefaultModels,
    type AppConfig,
    type AppearanceMode,
    type BackgroundAudioMode,
    type HistoryEntry,
    type OllamaDownloadEvent,
    type OllamaModelList,
    type OllamaModelSummary,
    type SectionId,
    type WhisperDownloadEvent,
    type WhisperModelList,
    type WhisperModelSummary,
    type UpdateStatus,
  } from './metadata';

  // ── Provider Metadata ─────────────────────────────────────────────────

  const userAgent = navigator.userAgent.toLowerCase();
  const isWindows = userAgent.includes('windows');
  const isMac = userAgent.includes('macintosh') || userAgent.includes('mac os');
  const hasNativeRuntime = isNativeRuntime();
  const defaultHotkey = isWindows ? 'capslock' : 'fn';
  const defaultTxProvider = isWindows ? 'localwhisper' : 'none';
  const buildLabel = `v${__APP_VERSION__} (${__GIT_COMMIT_SHORT__})`;
  const buildUrl = __GITHUB_COMMIT_URL__;

  const txProviders = transcriptionProviders(isWindows);
  const fmtProviderOptions = formattingProviders(isMac);

  // ── State ─────────────────────────────────────────────────────────────

  let loading = $state(true);
  let activeSection = $state<SectionId>('general');
  let configReady = $state(false);

  // General
  let hotkey = $state(defaultHotkey);
  let capturingHotkey = $state(false);
  let hotkeyPreview = $state('');
  let pressedHotkeyParts: string[] = [];
  let lastHotkeyPreview = '';
  let microphones = $state<string[]>([]);
  let selectedMic = $state('');
  let pressEnterAfterPaste = $state(false);

  // Transcription
  let txProvider = $state(defaultTxProvider);
  let txApiKey = $state('');
  let txModel = $state('');
  let txLanguage = $state('auto');
  let showTxApiKey = $state(false);
  let lastTxProvider = defaultTxProvider;
  let whisperModelList = $state<WhisperModelList | null>(null);
  let whisperModelsLoading = $state(false);
  let whisperModelsLoaded = $state(false);
  let whisperModelsError = $state('');
  let whisperSearchQuery = $state('');
  let whisperSearchResults = $state<WhisperModelSummary[]>([]);
  let whisperSearchLoading = $state(false);
  let whisperSearchError = $state('');
  let whisperDownloadEvent = $state<WhisperDownloadEvent | null>(null);
  let pendingWhisperUseAfterDownload = $state<string | null>(null);

  // Transcription provider options
  let dgSmartFormat = $state(true);
  let dgKeywords = $state('');
  let oaiPrompt = $state('');
  let geminiTemperature = $state(0);

  // Formatting
  let fmtProvider = $state('none');
  let fmtApiKey = $state('');
  let fmtModel = $state('');
  let fmtStyle = $state('formatted');
  let fmtCustomPrompt = $state('');
  let fmtUseSameKey = $state(true);
  let showFmtApiKey = $state(false);
  let lastFmtProvider = 'none';
  let ollamaModelList = $state<OllamaModelList | null>(null);
  let ollamaModelsLoading = $state(false);
  let ollamaModelsLoaded = $state(false);
  let ollamaModelsError = $state('');
  let ollamaSearchQuery = $state('');
  let ollamaSearchResults = $state<OllamaModelSummary[]>([]);
  let ollamaSearchLoading = $state(false);
  let ollamaSearchError = $state('');
  let ollamaSearchedQuery = $state('');
  let ollamaDownloadEvent = $state<OllamaDownloadEvent | null>(null);
  let pendingOllamaUseAfterDownload = $state<string | null>(null);

  // Behavior
  let soundsEnabled = $state(true);
  let appearanceMode = $state<AppearanceMode>('system');
  let backgroundAudioMode = $state<BackgroundAudioMode>('mute');
  let gradientEnabled = $state(true);
  let alwaysVisiblePill = $state(true);
  let startWithSystem = $state(false);

  // Load + sync autostart state with the OS
  async function loadAutostart() {
    if (!hasNativeRuntime) return;

    try {
      const enabled = await isAutostartEnabled();
      if (enabled !== null) {
        startWithSystem = enabled;
      }
    } catch (e) {
      console.error('Failed to load autostart state:', e);
    }
  }
  loadAutostart();

  async function toggleAutostart(enabled: boolean) {
    if (!hasNativeRuntime) return;

    try {
      const changed = await setAutostartEnabled(enabled);
      if (!changed) startWithSystem = !enabled;
    } catch (e) {
      startWithSystem = !enabled;
      console.error('Failed to update autostart state:', e);
    }
  }

  // History
  let historyEnabled = $state(true);
  let historyLoading = $state(true);
  let historyEntries = $state<HistoryEntry[]>([]);
  let copiedHistoryId = $state<string | null>(null);
  let historyLoadStarted = false;
  let copyTimeouts = new Map<string, ReturnType<typeof setTimeout>>();

  // Advanced
  let onboardingComplete = $state(false);
  let cacheCleaning = $state(false);
  let cacheMessage = $state('Remove the temporary recording and debug log from this device.');

  // Updates
  let updateStatus = $state<UpdateStatus>('idle');
  let updateMessage = $state('Check for signed Yap updates.');
  let updateVersion = $state('');
  let updateDownloaded = $state(0);
  let updateTotal = $state<number | null>(null);
  let updatePercent = $state<number | null>(null);
  let pendingUpdate: RuntimeUpdate | null = null;

  // ── Derived ───────────────────────────────────────────────────────────

  let hasTxProvider = $derived(txProvider !== 'none');
  let txProviderRequiresApiKey = $derived(transcriptionProviderRequiresApiKey(txProvider));
  let hasFmtProvider = $derived(fmtProvider !== 'none');
  let fmtProviderRequiresApiKey = $derived(formattingProviderRequiresApiKey(fmtProvider));
  let txModelLabel = $derived(txProviderRequiresApiKey ? 'Model' : 'Model Spec');
  let txModelDescription = $derived.by(() => {
    if (txProviderRequiresApiKey) {
      return `Override the default model, or leave empty to use ${txDefaultModels[txProvider] ?? 'none'}.`;
    }
    if (txProvider === 'localwhisper') {
      return 'Use a whisper.cpp GGML model path, or an id stored in Yap model folders such as base.en.';
    }
    return 'Provide a local model path or spec for this provider.';
  });

  let canShareApiKey = $derived.by(() => {
    if (!hasTxProvider || !hasFmtProvider || !fmtProviderRequiresApiKey) return false;
    return (
      (txProvider === 'gemini' && fmtProvider === 'gemini') ||
      (txProvider === 'openai' && fmtProvider === 'openai')
    );
  });

  let effectiveFmtApiKey = $derived(
    fmtUseSameKey && canShareApiKey ? txApiKey : fmtApiKey
  );

  let currentStyleData = $derived(styleData[fmtStyle] ?? styleData.formatted);
  let effectiveOllamaModelId = $derived(fmtModel.trim());
  let effectiveWhisperModelId = $derived(txModel.trim());
  let selectedWhisperModel = $derived.by(() => (
    whisperModelList?.models.find((model) => model.id === effectiveWhisperModelId || model.path === txModel.trim()) ?? null
  ));

  function languageOptionFor(value: string) {
    return languageOptions.find((option) => option.value === value) ?? languageOptions[0];
  }

  function languageValueFromConfig(cfg: AppConfig): string {
    const candidates = [
      cfg.oaiLanguage,
      cfg.dgLanguage,
      cfg.elLanguageCode,
      cfg.speechLocale,
    ].filter(Boolean);

    for (const candidate of candidates) {
      const normalized = candidate.toLowerCase();
      const match = languageOptions.find((option) => (
        option.providerCode === normalized ||
        option.speechLocale.toLowerCase() === normalized ||
        option.value === normalized
      ));
      if (match) return match.value;
    }

    return 'auto';
  }

  // ── Load Config ───────────────────────────────────────────────────────

  async function loadConfig() {
    if (!hasNativeRuntime) {
      loading = false;
      configReady = true;
      return;
    }

    configReady = false;
    try {
      const cfg = await invokeRuntimeOptional<AppConfig>('config.get', undefined, 2500);
      if (cfg) {
        hotkey = cfg.hotkey;
        selectedMic = cfg.audioDevice ?? '';
        pressEnterAfterPaste = cfg.pressEnterAfterPaste ?? false;
        txProvider = isWindows && cfg.txProvider === 'none' ? defaultTxProvider : cfg.txProvider;
        txApiKey = cfg.txApiKey;
        txModel = cfg.txModel;
        lastTxProvider = txProvider;
        txLanguage = languageValueFromConfig(cfg);
        fmtProvider = !isMac && cfg.fmtProvider === 'apple' ? 'none' : cfg.fmtProvider;
        fmtApiKey = cfg.fmtApiKey;
        fmtModel = cfg.fmtModel;
        lastFmtProvider = fmtProvider;
        fmtStyle = cfg.fmtStyle;
        fmtCustomPrompt = cfg.fmtCustomPrompt ?? '';
        onboardingComplete = cfg.onboardingComplete;
        dgSmartFormat = cfg.dgSmartFormat;
        dgKeywords = cfg.dgKeywords;
        oaiPrompt = cfg.oaiPrompt;
        geminiTemperature = cfg.geminiTemperature;
        soundsEnabled = cfg.soundsEnabled;
        appearanceMode = cfg.appearanceMode ?? 'system';
        backgroundAudioMode = cfg.backgroundAudioMode;
        gradientEnabled = cfg.gradientEnabled;
        alwaysVisiblePill = cfg.alwaysVisiblePill;
        historyEnabled = cfg.historyEnabled;

        // Determine if formatting shares the transcription key
        fmtUseSameKey = cfg.fmtApiKey === '' || cfg.fmtApiKey === cfg.txApiKey;
        savedConfigSnapshot = configSnapshot();
      } else {
        savedConfigSnapshot = configSnapshot();
      }

      void loadMicrophones();
    } catch (error) {
      console.error('Failed to load settings', error);
      savedConfigSnapshot = configSnapshot();
    } finally {
      loading = false;
      configReady = true;
    }
  }

  async function loadMicrophones() {
    if (!hasNativeRuntime) return;

    const devices = await invokeRuntimeOptional<string[]>('audio.list_devices', undefined, 2500);
    if (!devices) return;

    microphones = devices;
    if (selectedMic && !devices.includes(selectedMic)) {
      microphones = [selectedMic, ...devices];
    }
  }

  async function loadWhisperModels() {
    if (!hasNativeRuntime) return;

    whisperModelsLoading = true;
    whisperModelsError = '';
    try {
      const result = await invokeRuntimeOptional<WhisperModelList>('models.whisper.list', undefined, 10000);
      if (!result) {
        whisperModelsError = 'Could not load Local Whisper models.';
        whisperModelsLoaded = true;
        return;
      }
      whisperModelList = result;
      whisperModelsLoaded = true;
    } catch (error) {
      whisperModelsError = error instanceof Error ? error.message : String(error);
      whisperModelsLoaded = true;
      console.error('Failed to load Whisper models:', error);
    } finally {
      whisperModelsLoading = false;
    }
  }

  async function searchWhisperModels() {
    const query = whisperSearchQuery.trim();
    if (!hasNativeRuntime || query.length < 2) {
      whisperSearchResults = [];
      return;
    }

    whisperSearchLoading = true;
    whisperSearchError = '';
    try {
      whisperSearchResults = await invokeRuntime<WhisperModelSummary[]>('models.whisper.search', { query });
    } catch (error) {
      whisperSearchError = error instanceof Error ? error.message : String(error);
      whisperSearchResults = [];
      console.error('Failed to search Whisper models:', error);
    } finally {
      whisperSearchLoading = false;
    }
  }

  async function downloadWhisperModel(model: WhisperModelSummary, useAfterDownload = true) {
    if (!hasNativeRuntime || !model.url || isAnyWhisperDownloadActive()) return;

    whisperModelsError = '';
    whisperDownloadEvent = {
      id: model.id,
      fileName: model.fileName,
      status: 'started',
      total: model.sizeBytes,
    };
    pendingWhisperUseAfterDownload = useAfterDownload ? model.id : null;

    try {
      await invokeRuntime('models.whisper.download', {
        id: model.id,
        url: model.url,
        fileName: model.fileName,
        expectedSize: model.sizeBytes,
      });
    } catch (error) {
      whisperModelsError = error instanceof Error ? error.message : String(error);
      pendingWhisperUseAfterDownload = null;
      console.error('Failed to download Whisper model:', error);
    }
  }

  async function useWhisperModel(model: WhisperModelSummary) {
    txModel = model.id;
  }

  async function deleteWhisperModel(model: WhisperModelSummary) {
    if (!hasNativeRuntime || !model.installed) return;

    const confirmed = await confirmAction(
      `Delete ${model.name}? This only removes the cached model file from Yap.`,
      'Delete'
    );
    if (!confirmed) return;

    try {
      await invokeRuntime('models.whisper.delete', { fileName: model.fileName });
      if (txModel === model.id || txModel === model.path) {
        txModel = '';
      }
      whisperModelsLoaded = false;
      await loadWhisperModels();
    } catch (error) {
      whisperModelsError = error instanceof Error ? error.message : String(error);
      console.error('Failed to delete Whisper model:', error);
    }
  }

  async function revealWhisperModels() {
    if (!hasNativeRuntime) return;

    try {
      await invokeRuntime('models.whisper.reveal');
    } catch (error) {
      whisperModelsError = error instanceof Error ? error.message : String(error);
      console.error('Failed to reveal Whisper model folder:', error);
    }
  }

  function isSelectedWhisperModel(model: WhisperModelSummary): boolean {
    const modelValue = txModel.trim();
    if (!modelValue && model.id === txDefaultModels.localwhisper) return true;
    return modelValue === model.id || modelValue === model.path;
  }

  function isDownloadingWhisperModel(model: WhisperModelSummary): boolean {
    return (
      whisperDownloadEvent?.fileName === model.fileName &&
      (whisperDownloadEvent.status === 'started' || whisperDownloadEvent.status === 'progress')
    );
  }

  function isAnyWhisperDownloadActive(): boolean {
    return whisperDownloadEvent?.status === 'started' || whisperDownloadEvent?.status === 'progress';
  }

  function whisperDownloadProgress(model: WhisperModelSummary): number {
    if (!isDownloadingWhisperModel(model)) return 0;
    const event = whisperDownloadEvent;
    if (!event) return 0;
    if (event.percent !== undefined) return clampPercent(event.percent);
    if (!event.total || !event.transferred) return 0;
    return clampPercent((event.transferred / event.total) * 100);
  }

  function whisperDownloadLabel(model: WhisperModelSummary): string {
    const event = isDownloadingWhisperModel(model) ? whisperDownloadEvent : null;
    if (!event) return '';
    if (event.total && event.transferred !== undefined) {
      return `${formatBytes(event.transferred)} of ${formatBytes(event.total)}`;
    }
    if (event.transferred !== undefined) return formatBytes(event.transferred);
    return 'Starting';
  }

  function ollamaModelMeta(model: OllamaModelSummary): string {
    return [model.sizeLabel, model.libraryInfo, model.details].filter(Boolean).join(' · ');
  }

  function normalizeOllamaModel(id: string): string {
    return id.trim().toLowerCase().replace(/:latest$/, '');
  }

  function sameOllamaModel(a: string, b: string): boolean {
    return normalizeOllamaModel(a) === normalizeOllamaModel(b);
  }

  async function loadOllamaModels() {
    if (!hasNativeRuntime) return;
    ollamaModelsLoading = true;
    ollamaModelsError = '';
    try {
      const result = await invokeRuntimeOptional<OllamaModelList>('models.ollama.list', undefined, 10000);
      if (!result) {
        ollamaModelList = unavailableOllamaModelList('Could not load Ollama models.');
        ollamaModelsError = '';
        ollamaModelsLoaded = true;
        return;
      }
      ollamaModelList = result;
      ollamaModelsLoaded = true;
      ollamaModelsError = result.serviceError ?? '';
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      ollamaModelList = unavailableOllamaModelList(message);
      ollamaModelsError = '';
      ollamaModelsLoaded = true;
    } finally {
      ollamaModelsLoading = false;
    }
  }

  function unavailableOllamaModelList(message: string): OllamaModelList {
    return {
      serviceAvailable: false,
      serviceError: message,
      models: [],
    };
  }

  async function searchOllamaModels() {
    const query = ollamaSearchQuery.trim();
    if (query.length < 2 || ollamaSearchLoading || !hasNativeRuntime) return;
    ollamaSearchLoading = true;
    ollamaSearchError = '';
    ollamaSearchResults = [];
    ollamaSearchedQuery = '';
    try {
      const results = await invokeRuntime<OllamaModelSummary[]>('models.ollama.search', { query });
      if (query !== ollamaSearchQuery.trim()) return;
      ollamaSearchResults = results;
      ollamaSearchedQuery = query;
    } catch (error) {
      if (query !== ollamaSearchQuery.trim()) return;
      ollamaSearchError = error instanceof Error ? error.message : String(error);
    } finally {
      ollamaSearchLoading = false;
    }
  }

  async function pullOllamaModel(model: OllamaModelSummary, useAfterDownload = true) {
    if (!hasNativeRuntime || isAnyOllamaDownloadActive()) return;
    ollamaModelsError = '';
    ollamaDownloadEvent = {
      id: model.id,
      model: model.id,
      status: 'started',
    };
    pendingOllamaUseAfterDownload = useAfterDownload ? model.id : null;
    try {
      await invokeRuntime('models.ollama.pull', { id: model.id });
    } catch (error) {
      ollamaModelsError = error instanceof Error ? error.message : String(error);
      pendingOllamaUseAfterDownload = null;
      ollamaDownloadEvent = { id: model.id, model: model.id, status: 'error', error: ollamaModelsError };
    }
  }

  async function useOllamaModel(model: OllamaModelSummary) {
    fmtModel = model.id;
    await persistConfig();
  }

  async function deleteOllamaModel(model: OllamaModelSummary) {
    if (!hasNativeRuntime || !model.installed) return;
    const confirmed = window.confirm(
      `Delete ${model.name}? This removes it from your local Ollama models.`,
    );
    if (!confirmed) return;

    try {
      await invokeRuntime('models.ollama.delete', { id: model.id });
      if (sameOllamaModel(fmtModel, model.id)) {
        fmtModel = '';
      }
      ollamaModelsLoaded = false;
      await loadOllamaModels();
    } catch (error) {
      ollamaModelsError = error instanceof Error ? error.message : String(error);
    }
  }

  function isSelectedOllamaModel(model: OllamaModelSummary): boolean {
    return sameOllamaModel(model.id, effectiveOllamaModelId);
  }

  function isDownloadingOllamaModel(model: OllamaModelSummary): boolean {
    return (
      ollamaDownloadEvent?.id === model.id &&
      ['started', 'progress'].includes(ollamaDownloadEvent.status)
    );
  }

  function isAnyOllamaDownloadActive(): boolean {
    return !!ollamaDownloadEvent && ['started', 'progress'].includes(ollamaDownloadEvent.status);
  }

  function ollamaDownloadProgress(model: OllamaModelSummary): number {
    if (!isDownloadingOllamaModel(model)) return 0;
    return Math.round(ollamaDownloadEvent?.percent ?? 0);
  }

  function ollamaDownloadLabel(model: OllamaModelSummary): string {
    const event = isDownloadingOllamaModel(model) ? ollamaDownloadEvent : null;
    if (!event) return '';
    if (event.message) return event.message;
    if (event.total && event.transferred !== undefined) {
      return `${formatBytes(event.transferred)} of ${formatBytes(event.total)}`;
    }
    return 'Starting';
  }

  async function refreshConfig() {
    if (loading) return;
    await loadConfig();
  }

  // ── Save Config ───────────────────────────────────────────────────────

  function currentConfig(): AppConfig {
    const language = languageOptionFor(txLanguage);

    return {
      hotkey,
      audioDevice: selectedMic,
      pressEnterAfterPaste,
      txProvider,
      txApiKey,
      txModel,
      fmtProvider,
      fmtApiKey: fmtUseSameKey && canShareApiKey ? '' : fmtApiKey,
      fmtModel,
      fmtStyle,
      fmtCustomPrompt,
      onboardingComplete,
      dgSmartFormat,
      dgKeywords,
      dgLanguage: language.providerCode,
      oaiLanguage: language.providerCode,
      oaiPrompt,
      geminiTemperature,
      elLanguageCode: language.providerCode,
      soundsEnabled,
      appearanceMode,
      backgroundAudioMode,
      gradientEnabled,
      alwaysVisiblePill,
      historyEnabled,
      speechLocale: language.speechLocale,
    };
  }

  async function persistConfig() {
    if (!hasNativeRuntime) return;
    const snapshot = configSnapshot();
    if (snapshot === savedConfigSnapshot) return;

    try {
      await invokeRuntime('config.save', { config: currentConfig() });
      savedConfigSnapshot = snapshot;
    } catch (e) {
      console.error('Failed to save config:', e);
    }
  }

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let savedConfigSnapshot = '';

  function configSnapshot() {
    return JSON.stringify(currentConfig());
  }

  function scheduleSave() {
    if (!hasNativeRuntime || !configReady || loading) return;
    if (configSnapshot() === savedConfigSnapshot) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      void persistConfig();
    }, 300);
  }

  $effect(() => {
    hotkey;
    selectedMic;
    pressEnterAfterPaste;
    txProvider;
    txApiKey;
    txModel;
    txLanguage;
    fmtProvider;
    fmtApiKey;
    fmtModel;
    fmtStyle;
    fmtCustomPrompt;
    onboardingComplete;
    dgSmartFormat;
    dgKeywords;
    oaiPrompt;
    geminiTemperature;
    soundsEnabled;
    appearanceMode;
    backgroundAudioMode;
    gradientEnabled;
    alwaysVisiblePill;
    historyEnabled;
    fmtUseSameKey;

    scheduleSave();
  });

  $effect(() => {
    if (txProvider === 'localwhisper' && !whisperModelsLoaded && !whisperModelsLoading) {
      void loadWhisperModels();
    }
  });

  $effect(() => {
    if (fmtProvider === 'ollama' && !ollamaModelsLoaded && !ollamaModelsLoading) {
      void loadOllamaModels();
    }
  });

  $effect(() => {
    if (!configReady || loading || txProvider === lastTxProvider) return;
    lastTxProvider = txProvider;
    txModel = '';
  });

  $effect(() => {
    if (!configReady || loading || fmtProvider === lastFmtProvider) return;
    lastFmtProvider = fmtProvider;
    fmtModel = '';
  });

  // ── Close Window ──────────────────────────────────────────────────────

  async function closeWindow() {
    if (capturingHotkey) {
      if (hasNativeRuntime) {
        await invokeRuntimeOptional('hotkey_capture.cancel');
      }
    }
    if (hasNativeRuntime) {
      await hideWindow('settings');
    }
  }

  async function openBuildLink(event: MouseEvent) {
    event.preventDefault();
    try {
      await openExternal(buildUrl);
    } catch (e) {
      console.error('Failed to open build link:', e);
    }
  }

  async function openOllamaDownload() {
    try {
      await openExternal('https://ollama.com/download');
    } catch (e) {
      console.error('Failed to open Ollama download:', e);
    }
  }

  function selectSection(section: SectionId) {
    if (capturingHotkey) {
      cancelHotkeyCapture();
    }
    activeSection = section;
    if (section === 'history') {
      void loadHistory();
    }
  }

  async function toggleHotkeyCapture() {
    hotkeyPreview = '';
    resetHotkeyCaptureState();
    capturingHotkey = !capturingHotkey;

    if (capturingHotkey) {
      if (hasNativeRuntime) {
        await invokeRuntimeOptional('hotkey_capture.start');
      }
    } else {
      if (hasNativeRuntime) {
        await invokeRuntimeOptional('hotkey_capture.cancel');
      }
    }
  }

  function setCapturedHotkey(value: string) {
    if (shortcutIncludesEscape(value)) {
      cancelHotkeyCapture();
      return;
    }

    hotkey = value;
    hotkeyPreview = '';
    resetHotkeyCaptureState();
    capturingHotkey = false;
    if (hasNativeRuntime) {
      void invokeRuntimeOptional('hotkey_capture.cancel');
    }
  }

  function cancelHotkeyCapture() {
    hotkeyPreview = '';
    resetHotkeyCaptureState();
    capturingHotkey = false;
    if (hasNativeRuntime) {
      void invokeRuntimeOptional('hotkey_capture.cancel');
    }
  }

  function shortcutIncludesEscape(value: string): boolean {
    return value.split('+').filter(Boolean).includes('escape');
  }

  // ── Keyboard ──────────────────────────────────────────────────────────

  function onKeyDown(e: KeyboardEvent) {
    if (capturingHotkey) {
      e.preventDefault();
      e.stopPropagation();

      if (e.key === 'Escape') {
        cancelHotkeyCapture();
        return;
      }

      const key = canonicalKeyFromEvent(e);
      syncModifiers(e);
      if (key) addHotkeyPart(key);

      const preview = hotkeyFromPressed();
      if (preview) {
        lastHotkeyPreview = preview;
        hotkeyPreview = preview;
      }
      return;
    }

    if (e.key === 'Escape') {
      closeWindow();
    }
  }

  function onKeyUp(e: KeyboardEvent) {
    if (!capturingHotkey) return;
    e.preventDefault();
    e.stopPropagation();

    const key = canonicalKeyFromEvent(e);
    if (key) removeHotkeyPart(key);
    syncModifiers(e);

    if (pressedHotkeyParts.length === 0 && lastHotkeyPreview) {
      setCapturedHotkey(lastHotkeyPreview);
    }
  }

  function resetHotkeyCaptureState() {
    pressedHotkeyParts = [];
    lastHotkeyPreview = '';
  }

  function addHotkeyPart(part: string) {
    if (!pressedHotkeyParts.includes(part)) {
      pressedHotkeyParts = [...pressedHotkeyParts, part];
    }
  }

  function removeHotkeyPart(part: string) {
    pressedHotkeyParts = pressedHotkeyParts.filter((pressed) => pressed !== part);
  }

  function syncModifiers(e: KeyboardEvent) {
    syncModifier('cmd', e.metaKey);
    syncModifier('ctrl', e.ctrlKey);
    syncModifier('option', e.altKey);
    syncModifier('shift', e.shiftKey);
  }

  function syncModifier(part: string, pressed: boolean) {
    if (pressed) {
      addHotkeyPart(part);
    } else {
      removeHotkeyPart(part);
    }
  }

  function hotkeyFromPressed(): string {
    const modifiers = modifierOrder.filter((modifier) => pressedHotkeyParts.includes(modifier));
    const triggers = pressedHotkeyParts.filter((part) => !modifierOrder.includes(part));
    return [...modifiers, ...triggers].join('+');
  }

  function canonicalKeyFromEvent(e: KeyboardEvent): string {
    if (e.key === 'Meta' || e.code === 'MetaLeft' || e.code === 'MetaRight') return 'cmd';
    if (e.key === 'Control' || e.code === 'ControlLeft' || e.code === 'ControlRight') return 'ctrl';
    if (e.key === 'Alt' || e.key === 'Option' || e.code === 'AltLeft' || e.code === 'AltRight') return 'option';
    if (e.key === 'Shift' || e.code === 'ShiftLeft' || e.code === 'ShiftRight') return 'shift';
    if (e.key === 'Fn' || e.key === 'fn' || e.key === 'F24') return 'fn';
    if (e.code.startsWith('Key')) return e.code.slice(3).toLowerCase();
    if (e.code.startsWith('Digit')) return e.code.slice(5);
    if (e.code.startsWith('Numpad') && e.code.length === 7) return e.code.slice(6);
    if (e.code.startsWith('F') && /^F\d{1,2}$/.test(e.code)) return e.code.toLowerCase();

    const namedKeys: Record<string, string> = {
      Space: 'space',
      Enter: 'return',
      Return: 'return',
      Tab: 'tab',
      Escape: 'escape',
      Backspace: 'delete',
      Delete: 'forwarddelete',
      CapsLock: 'capslock',
      ArrowLeft: 'left',
      ArrowRight: 'right',
      ArrowUp: 'up',
      ArrowDown: 'down',
      Home: 'home',
      End: 'end',
      PageUp: 'pageup',
      PageDown: 'pagedown',
      Semicolon: ';',
      Equal: '=',
      Comma: ',',
      Minus: '-',
      Period: '.',
      Slash: '/',
      Backquote: '`',
      BracketLeft: '[',
      Backslash: '\\',
      BracketRight: ']',
      Quote: "'",
    };

    if (namedKeys[e.code]) return namedKeys[e.code];
    if (e.key.length === 1) return e.key.toLowerCase();
    return '';
  }

  // ── Hotkey Display ────────────────────────────────────────────────────

  function hotkeyDisplayParts(key: string): string[] {
    return key
      .split('+')
      .filter(Boolean)
      .map(hotkeyDisplayPartLabel);
  }

  function hotkeyDisplayLabel(key: string): string {
    return key
      .split('+')
      .filter(Boolean)
      .map(hotkeyDisplayPartLabel)
      .join('+');
  }

  function hotkeyDisplayPartLabel(part: string): string {
    if (part === 'cmd') return 'Cmd';
    if (part === 'ctrl') return 'Ctrl';
    if (part === 'option') return 'Option';
    if (part === 'shift') return 'Shift';
    if (part === 'fn') return 'fn';
    if (part === 'space') return 'Space';
    if (part === 'return') return 'Return';
    if (part === 'escape') return 'Esc';
    if (part === 'delete') return 'Delete';
    if (part === 'forwarddelete') return 'Forward Delete';
    if (part === 'capslock') return 'Caps Lock';
    if (part === 'pageup') return 'Page Up';
    if (part === 'pagedown') return 'Page Down';
    if (part === 'left') return 'Left';
    if (part === 'right') return 'Right';
    if (part === 'up') return 'Up';
    if (part === 'down') return 'Down';
    if (part.startsWith('keycode:')) return `Key ${part.slice('keycode:'.length)}`;
    if (part.startsWith('vk:')) return `Key ${part.slice('vk:'.length)}`;
    if (part.length === 1) return part.toUpperCase();
    if (/^f\d{1,2}$/.test(part)) return part.toUpperCase();
    return part;
  }

  // ── History Entries ──────────────────────────────────────────────────

  async function loadHistory() {
    historyLoadStarted = true;
    historyLoading = true;

    if (!hasNativeRuntime) {
      historyEntries = [];
      historyLoading = false;
      return;
    }

    try {
      historyEntries = await invokeRuntimeOptional<HistoryEntry[]>('history.get', undefined, 2500) ?? [];
    } finally {
      historyLoading = false;
    }
  }

  function providerLabel(tx: string, fmt: string | null): string {
    const txLabel = providerLabels[tx] ?? tx;
    if (!fmt || fmt === 'none') return txLabel;
    const fmtLabel = providerLabels[fmt] ?? fmt;
    if (txLabel === fmtLabel) return txLabel;
    return `${txLabel} + ${fmtLabel}`;
  }

  function hasHistoryDetails(entry: HistoryEntry): boolean {
    return Boolean(
      entry.rawText ||
      entry.formattedText ||
      entry.transcriptionModel ||
      entry.formattingModel ||
      entry.formattingInstruction
    );
  }

  function historyModelLabel(entry: HistoryEntry): string | null {
    const models = [
      entry.transcriptionModel ? `Transcription: ${entry.transcriptionModel}` : null,
      entry.formattingModel ? `Formatting: ${entry.formattingModel}` : null,
    ].filter(Boolean);

    return models.length ? models.join(' · ') : null;
  }

  function historyInstructionLabel(entry: HistoryEntry): string {
    if (entry.formattingStyle === 'custom') return 'Custom instruction';
    return 'Formatter instruction';
  }

  function relativeTime(isoString: string): string {
    const now = Date.now();
    const then = new Date(isoString).getTime();
    const diffMs = now - then;
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHr = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHr / 24);

    if (Number.isNaN(then)) return '';
    if (diffSec < 10) return 'just now';
    if (diffSec < 60) return `${diffSec}s ago`;
    if (diffMin < 60) return `${diffMin}m ago`;
    if (diffHr < 24) return `${diffHr}h ago`;
    if (diffDay === 1) return 'Yesterday';
    if (diffDay < 7) return `${diffDay}d ago`;
    if (diffDay < 30) return `${Math.floor(diffDay / 7)}w ago`;
    return new Date(isoString).toLocaleDateString();
  }

  async function copyHistoryEntry(entry: HistoryEntry) {
    try {
      await navigator.clipboard.writeText(entry.text);

      const existing = copyTimeouts.get(entry.id);
      if (existing) clearTimeout(existing);

      copiedHistoryId = entry.id;
      const timeout = setTimeout(() => {
        if (copiedHistoryId === entry.id) copiedHistoryId = null;
      }, 1500);
      copyTimeouts.set(entry.id, timeout);
    } catch (e) {
      console.error('Failed to copy history entry:', e);
    }
  }

  async function deleteHistoryEntry(id: string) {
    if (!hasNativeRuntime) return;

    try {
      await invokeRuntime('history.remove', { id });
      historyEntries = historyEntries.filter((entry) => entry.id !== id);
      void invokeRuntimeOptional('history_menu.refresh');
    } catch (e) {
      console.error('Failed to delete history entry:', e);
    }
  }

  async function clearHistoryEntries() {
    const count = historyEntries.length;
    const confirmed = await confirmAction(
      `Clear all history? This will permanently delete ${count} transcription ${count === 1 ? 'entry' : 'entries'}.`,
      'Clear'
    );
    if (!confirmed || !hasNativeRuntime) return;

    try {
      await invokeRuntime('history.clear');
      historyEntries = [];
      void invokeRuntimeOptional('history_menu.refresh');
    } catch (e) {
      console.error('Failed to clear history:', e);
    }
  }

  async function cleanCache() {
    const confirmed = await confirmAction(
      'Clean local cache? This deletes Yap\'s temporary recording and debug log. It does not delete transcription history or downloaded models.',
      'Clean'
    );
    if (!confirmed || !hasNativeRuntime || cacheCleaning) return;

    cacheCleaning = true;
    try {
      const result = await invokeRuntime<{ removed: string[]; missing: string[] }>('cache.clear');
      if (result.removed.length > 0) {
        cacheMessage = `Removed ${result.removed.join(' and ')}.`;
      } else {
        cacheMessage = 'No temporary recording or debug log was found.';
      }
    } catch (e) {
      cacheMessage = e instanceof Error ? e.message : 'Failed to clean cache.';
      console.error('Failed to clean cache:', e);
    } finally {
      cacheCleaning = false;
    }
  }

  // ── Updates ───────────────────────────────────────────────────────────

  function updateBusy(): boolean {
    return updateStatus === 'checking' || updateStatus === 'downloading' || updateStatus === 'ready';
  }

  function updateProgress(): number {
    if (updateStatus === 'ready') return 100;
    if (updatePercent !== null) return clampPercent(updatePercent);
    if (!updateTotal || updateTotal <= 0) return 0;
    return clampPercent((updateDownloaded / updateTotal) * 100);
  }

  function updateProgressLabel(): string {
    if (updateStatus === 'ready') return 'Installing';
    if (!updateTotal && updatePercent !== null) return `${updateProgress()}%`;
    if (!updateTotal) return formatBytes(updateDownloaded);
    return `${formatBytes(updateDownloaded)} of ${formatBytes(updateTotal)}`;
  }

  function clampPercent(value: number): number {
    return Math.max(0, Math.min(100, Math.round(value)));
  }

  function positiveNumber(value: unknown): number | null {
    return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : null;
  }

  function nonNegativeNumber(value: unknown): number | null {
    return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
  }

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return '0 MB';
    const megabytes = bytes / 1024 / 1024;
    if (megabytes < 10) return `${megabytes.toFixed(1)} MB`;
    return `${Math.round(megabytes)} MB`;
  }

  function updaterErrorMessage(error: unknown): string {
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes('latest-mac.yml') || message.includes('latest.yml') || message.includes('update metadata')) {
      return 'No signed update metadata is available for this release.';
    }
    if (message.includes('signature')) {
      return 'The update could not be verified, so Yap did not install it.';
    }
    return message || 'Update failed.';
  }

  async function checkForUpdates() {
    if (!hasNativeRuntime || updateBusy()) return;

    pendingUpdate = null;
    updateVersion = '';
    updateDownloaded = 0;
    updateTotal = null;
    updatePercent = null;
    updateStatus = 'checking';
    updateMessage = 'Checking for updates...';

    try {
      const update = await checkForRuntimeUpdate({ timeout: 30000 });

      if (!update) {
        updateStatus = 'upToDate';
        updateMessage = 'Yap is up to date.';
        return;
      }

      pendingUpdate = update;
      updateVersion = update.version;
      updateStatus = 'available';
      updateMessage = `Yap ${update.version} is ready to install.`;
    } catch (error) {
      pendingUpdate = null;
      updateStatus = 'error';
      updateMessage = updaterErrorMessage(error);
      console.error('Failed to check for updates:', error);
    }
  }

  async function installUpdate() {
    if (!pendingUpdate || updateBusy()) return;

    const version = pendingUpdate.version;
    const confirmed = await confirmAction(
      `Install Yap ${version}? Yap will restart after the update finishes.`,
      'Install'
    );
    if (!confirmed || !pendingUpdate) return;

    updateStatus = 'downloading';
    updateDownloaded = 0;
    updateTotal = null;
    updatePercent = null;
    updateMessage = `Downloading Yap ${version}...`;

    try {
      const update = pendingUpdate;
      await update.downloadAndInstall((event: RuntimeDownloadEvent) => {
        const data = event.data ?? {};
        if (event.event === 'Started') {
          updateTotal = positiveNumber(data.total ?? data.contentLength);
          updateDownloaded = 0;
          updatePercent = null;
        } else if (event.event === 'Progress') {
          updateTotal = positiveNumber(data.total ?? data.contentLength) ?? updateTotal;
          updateDownloaded =
            nonNegativeNumber(data.transferred) ?? updateDownloaded + (data.chunkLength ?? 0);
          updatePercent = nonNegativeNumber(data.percent) ?? updatePercent;
        } else if (event.event === 'Finished') {
          updateTotal = positiveNumber(data.total ?? data.contentLength) ?? updateTotal;
          updateDownloaded = nonNegativeNumber(data.transferred) ?? updateDownloaded;
          updatePercent = 100;
          updateStatus = 'ready';
          updateMessage = 'Download finished. Installing update...';
        }
      });
    } catch (error) {
      updateStatus = 'error';
      updateMessage = updaterErrorMessage(error);
      console.error('Failed to install update:', error);
    }
  }

  async function confirmAction(message: string, okLabel: string): Promise<boolean> {
    return confirmRuntime(message, {
      title: 'Yap',
      kind: 'warning',
      okLabel,
      cancelLabel: 'Cancel',
    });
  }

  async function confirmReset(message: string): Promise<boolean> {
    return confirmAction(message, 'Reset');
  }

  async function resetDefaults() {
    const confirmed = await confirmReset(
      'Reset settings to defaults? This will restore every setting in this window and turn off Start with system.'
    );
    if (!confirmed) return;

    hotkey = defaultHotkey;
    selectedMic = '';
    pressEnterAfterPaste = false;
    txProvider = defaultTxProvider;
    txApiKey = '';
    txModel = '';
    txLanguage = 'auto';
    fmtProvider = 'none';
    fmtApiKey = '';
    fmtModel = '';
    fmtStyle = 'casual';
    fmtCustomPrompt = '';
    onboardingComplete = false;
    dgSmartFormat = true;
    dgKeywords = '';
    oaiPrompt = '';
    geminiTemperature = 0;
    soundsEnabled = true;
    appearanceMode = 'system';
    backgroundAudioMode = 'mute';
    gradientEnabled = true;
    alwaysVisiblePill = true;
    startWithSystem = false;
    void toggleAutostart(false);
    historyEnabled = true;
    void persistConfig();
  }

  // ── Init ──────────────────────────────────────────────────────────────

  // Load config immediately on mount.
  loadConfig();

  // Re-load config whenever the settings window is shown / focused, so
  // the form always reflects the latest persisted values (the window is
  // hidden rather than destroyed when closed).
  let unlistenFocus: (() => void) | undefined;
  let unlistenHotkeyPreview: (() => void) | undefined;
  let unlistenHotkeyCapture: (() => void) | undefined;
  let unlistenShowSection: (() => void) | undefined;
  let unlistenShowHistory: (() => void) | undefined;
  let unlistenShowUpdates: (() => void) | undefined;
  let unlistenHistoryCleared: (() => void) | undefined;
  let unlistenWhisperDownload: (() => void) | undefined;
  let unlistenOllamaDownload: (() => void) | undefined;

  if (hasNativeRuntime) {
    onRuntimeFocusChanged((focused) => {
      if (focused) {
        void refreshConfig();
        if (txProvider === 'localwhisper') {
          whisperModelsLoaded = false;
          void loadWhisperModels();
        }
        if (fmtProvider === 'ollama') {
          ollamaModelsLoaded = false;
          void loadOllamaModels();
        }
        if (activeSection === 'history' || historyLoadStarted) {
          void loadHistory();
        }
      }
    })
      .then((fn) => {
        unlistenFocus = fn;
      });

    listenRuntimeEvent<string>('settings:hotkey-preview', (payload) => {
      if (capturingHotkey) {
        if (shortcutIncludesEscape(payload)) {
          cancelHotkeyCapture();
          return;
        }
        hotkeyPreview = payload;
      }
    })
      .then((fn) => {
        unlistenHotkeyPreview = fn;
      });

    listenRuntimeEvent<string>('settings:hotkey-captured', (payload) => {
      setCapturedHotkey(payload);
    })
      .then((fn) => {
        unlistenHotkeyCapture = fn;
      });

    listenRuntimeEvent<SectionId>('settings:show-section', (payload) => {
      if (settingsSections.some((section) => section.id === payload)) {
        activeSection = payload;
        if (payload === 'history') {
          void loadHistory();
        }
      }
    })
      .then((fn) => {
        unlistenShowSection = fn;
      });

    listenRuntimeEvent('settings:show-history', () => {
      activeSection = 'history';
      void loadHistory();
    })
      .then((fn) => {
        unlistenShowHistory = fn;
      });

    listenRuntimeEvent('settings:show-updates', () => {
      activeSection = 'advanced';
      void checkForUpdates();
    })
      .then((fn) => {
        unlistenShowUpdates = fn;
      });

    listenRuntimeEvent('tray:history-cleared', () => {
      if (activeSection === 'history' || historyLoadStarted) {
        void loadHistory();
      }
    })
      .then((fn) => {
        unlistenHistoryCleared = fn;
      });

    listenRuntimeEvent<WhisperDownloadEvent>('models:download', (payload) => {
      whisperDownloadEvent = payload;
      if (payload.status === 'finished') {
        if (txProvider === 'localwhisper' && pendingWhisperUseAfterDownload === payload.id) {
          txModel = payload.id;
        }
        pendingWhisperUseAfterDownload = null;
        whisperModelsLoaded = false;
        void loadWhisperModels();
      } else if (payload.status === 'error') {
        whisperModelsError = payload.error ?? 'Model download failed.';
        pendingWhisperUseAfterDownload = null;
      }
    })
      .then((fn) => {
        unlistenWhisperDownload = fn;
      });

    listenRuntimeEvent<OllamaDownloadEvent>('models:ollama-download', (payload) => {
      ollamaDownloadEvent = payload;
      if (payload.status === 'finished') {
        if (fmtProvider === 'ollama' && pendingOllamaUseAfterDownload === payload.id) {
          fmtModel = payload.id;
        }
        pendingOllamaUseAfterDownload = null;
        ollamaModelsLoaded = false;
        void loadOllamaModels();
      } else if (payload.status === 'error') {
        ollamaModelsError = payload.error ?? 'Ollama model download failed.';
        pendingOllamaUseAfterDownload = null;
      }
    })
      .then((fn) => {
        unlistenOllamaDownload = fn;
      });
  }

  onDestroy(() => {
    unlistenFocus?.();
    unlistenHotkeyPreview?.();
    unlistenHotkeyCapture?.();
    unlistenShowSection?.();
    unlistenShowHistory?.();
    unlistenShowUpdates?.();
    unlistenHistoryCleared?.();
    unlistenWhisperDownload?.();
    unlistenOllamaDownload?.();
    if (saveTimer) clearTimeout(saveTimer);
    for (const timeout of copyTimeouts.values()) {
      clearTimeout(timeout);
    }
    if (hasNativeRuntime) {
      void invokeRuntimeOptional('hotkey_capture.cancel');
    }
  });
</script>

<svelte:window onkeydown={onKeyDown} onkeyup={onKeyUp} />

{#if loading}
  <div class="settings-container loading-state" data-appearance-mode={appearanceMode}>
    <span>Loading...</span>
  </div>
{:else}
  <div class="settings-container" class:platform-macos={isMac} data-appearance-mode={appearanceMode}>
    <div class="settings-titlebar" aria-hidden="true"></div>
    <div class="settings-body">
      <aside class="settings-sidebar" aria-label="Settings sections">
        <div class="sidebar-header">
          <img class="app-icon" src="./favicon.png" alt="" aria-hidden="true" />
          <div>
            <div class="sidebar-title">Yap</div>
            <a
              class="sidebar-subtitle sidebar-version-link"
              href={buildUrl}
              onclick={openBuildLink}
              target="_blank"
              rel="noreferrer"
              title="Open this build on GitHub"
            >
              {buildLabel}
            </a>
          </div>
        </div>

        <nav class="section-nav">
          {#each settingsSections as section}
            <button
              class="section-nav-item"
              class:active={activeSection === section.id}
              type="button"
              aria-current={activeSection === section.id ? 'page' : undefined}
              aria-controls={'settings-panel-' + section.id}
              onclick={() => selectSection(section.id)}
            >
              <span class="section-nav-label">{section.label}</span>
              <span class="section-nav-description">{section.description}</span>
            </button>
          {/each}
        </nav>

      </aside>

      <div class="settings-main">
        <main class="settings-content" id={'settings-panel-' + activeSection}>
        {#if activeSection === 'general'}
          <section class="settings-section" aria-label="General settings">
            <div class="section-body">
              <div class="field-row hotkey-field">
                <div class="field-copy">
                  <span class="field-label">Hotkey</span>
                </div>
                <button
                  class="hotkey-button"
                  class:capturing={capturingHotkey}
                  onclick={toggleHotkeyCapture}
                  type="button"
                  aria-label={capturingHotkey ? 'Press shortcut' : `Current hotkey: ${hotkeyDisplayLabel(hotkey)}`}
                >
                  {#if capturingHotkey}
                    {#if hotkeyPreview}
                      <span class="keycap-stack" aria-hidden="true">
                        {#each hotkeyDisplayParts(hotkeyPreview) as part, index}
                          <span class="keycap keycap-live">{part}</span>
                          {#if index < hotkeyDisplayParts(hotkeyPreview).length - 1}
                            <span class="keycap-plus" aria-hidden="true">+</span>
                          {/if}
                        {/each}
                      </span>
                      <span class="sr-only">{hotkeyDisplayLabel(hotkeyPreview)}</span>
                    {:else}
                      <span class="hotkey-placeholder">Press shortcut...</span>
                    {/if}
                  {:else}
                    <span class="keycap-stack" aria-hidden="true">
                      {#each hotkeyDisplayParts(hotkey) as part, index}
                        <span class="keycap">{part}</span>
                        {#if index < hotkeyDisplayParts(hotkey).length - 1}
                          <span class="keycap-plus" aria-hidden="true">+</span>
                        {/if}
                      {/each}
                    </span>
                    <span class="sr-only">{hotkeyDisplayLabel(hotkey)}</span>
                  {/if}
                </button>
                <span class="field-description">Set the global keyboard shortcut.</span>
              </div>

              <div class="field-row">
                <span class="field-label">Microphone</span>
                <div class="select-wrapper">
                  <select class="select" bind:value={selectedMic}>
                    <option value="">System Default</option>
                    {#each microphones as mic}
                      <option value={mic}>{mic}</option>
                    {/each}
                    {#if microphones.length === 0}
                      <option value="">No devices found</option>
                    {/if}
                  </select>
                  <button class="select-toggle" aria-hidden="true" tabindex="-1" type="button">
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8">
                      <path d="M4 6L8 10L12 6" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                  </button>
                </div>
              </div>

              <div class="field-divider"></div>

              <div class="toggle-row">
                <div class="toggle-info">
                  <span class="toggle-label">Start with system</span>
                  <span class="toggle-description">Start Yap automatically after login.</span>
                </div>
                <label class="toggle-switch">
                  <input type="checkbox" bind:checked={startWithSystem} onchange={() => { void toggleAutostart(startWithSystem); }} />
                  <span class="toggle-track"></span>
                  <span class="toggle-thumb"></span>
                </label>
              </div>

              <div class="field-divider"></div>

              <div class="toggle-row">
                <div class="toggle-info">
                  <span class="toggle-label">Press Enter after paste</span>
                  <span class="toggle-description">Paste the transcription, then press Enter.</span>
                </div>
                <label class="toggle-switch">
                  <input type="checkbox" bind:checked={pressEnterAfterPaste} />
                  <span class="toggle-track"></span>
                  <span class="toggle-thumb"></span>
                </label>
              </div>

              <div class="field-divider"></div>

              <div class="toggle-row">
                <div class="toggle-info">
                  <span class="toggle-label">Sound effects</span>
                  <span class="toggle-description">Play sounds for recording, completion, and errors.</span>
                </div>
                <label class="toggle-switch">
                  <input type="checkbox" bind:checked={soundsEnabled} />
                  <span class="toggle-track"></span>
                  <span class="toggle-thumb"></span>
                </label>
              </div>

              <div class="field-divider"></div>

              <div class="field-row split">
                <div class="field-copy">
                  <span class="field-label">Appearance</span>
                  <span class="field-description">Match the system or choose a fixed theme.</span>
                </div>
                <div class="segmented-control" role="radiogroup" aria-label="Appearance">
                  {#each appearanceModes as mode}
                    <label class="segment-option">
                      <input type="radio" bind:group={appearanceMode} value={mode.value} />
                      <span>{mode.label}</span>
                    </label>
                  {/each}
                </div>
              </div>

              <div class="field-divider"></div>

              <div class="field-row split">
                <div class="field-copy">
                  <span class="field-label">Background audio</span>
                  <span class="field-description">Choose what happens to other media while recording.</span>
                </div>
                <div class="segmented-control" role="radiogroup" aria-label="Background audio">
                  {#each backgroundAudioModes as mode}
                    <label class="segment-option">
                      <input type="radio" bind:group={backgroundAudioMode} value={mode.value} />
                      <span>{mode.label}</span>
                    </label>
                  {/each}
                </div>
              </div>

              <div class="field-divider"></div>

              <div class="toggle-row">
                <div class="toggle-info">
                  <span class="toggle-label">Gradient background</span>
                  <span class="toggle-description">Show the animated background while recording.</span>
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
            </div>
          </section>
        {/if}

        {#if activeSection === 'transcription'}
          <section class="settings-section" aria-label="Transcription settings">
            <div class="section-body">
              <div class="field-row">
                <span class="field-label">Provider</span>
                <div class="select-wrapper">
                  <select class="select" bind:value={txProvider}>
                    {#each txProviders as p}
                      <option value={p.value} disabled={p.disabled}>{p.label}</option>
                    {/each}
                  </select>
                  <button class="select-toggle" aria-hidden="true" tabindex="-1" type="button">
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8">
                      <path d="M4 6L8 10L12 6" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                  </button>
                </div>
                {#if !hasTxProvider}
                  <span class="field-description">
                    {isWindows
                      ? 'Choose an API provider to enable transcription.'
                      : 'Uses Apple SpeechAnalyzer for on-device transcription on macOS 26 or newer. Choose an API provider for cloud transcription.'}
                  </span>
                {/if}
              </div>

              <div class="field-row">
                <span class="field-label">Language</span>
                <div class="select-wrapper">
                  <select class="select" bind:value={txLanguage}>
                    {#each languageOptions as language}
                      <option value={language.value}>{language.label}</option>
                    {/each}
                  </select>
                  <button class="select-toggle" aria-hidden="true" tabindex="-1" type="button">
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8">
                      <path d="M4 6L8 10L12 6" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                  </button>
                </div>
                <span class="field-description">Use auto-detect, or choose a language to improve recognition.</span>
              </div>

              {#if hasTxProvider}
                <div class="field-divider"></div>

                {#if txProviderRequiresApiKey}
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
                {/if}

                {#if txProvider === 'localwhisper'}
                  <div class="field-row">
                    <div class="local-models-header">
                      <div class="field-copy">
                        <span class="field-label">Local Models</span>
                        <span class="field-description">
                          Whisper models are stored in Yap's cache and are not bundled with the app.
                        </span>
                      </div>
                      <button class="btn" type="button" onclick={revealWhisperModels}>Reveal</button>
                    </div>

                    {#if whisperModelsLoading && !whisperModelList}
                      <div class="history-empty-state">
                        <span>Loading models...</span>
                      </div>
                    {:else if whisperModelList && whisperModelList.models.length === 0}
                      <div class="history-empty-state">
                        <span>No models installed. Search Hugging Face below to download one, or enter a model path.</span>
                      </div>
                    {:else if whisperModelList}
                      <div class="model-list">
                        {#each whisperModelList.models as model}
                          <div class="model-row" class:selected={isSelectedWhisperModel(model)}>
                            <div class="model-main">
                              <div class="model-title-row">
                                <span class="model-name">{model.name}</span>
                                {#if model.installed}
                                  <span class="model-status installed">Installed</span>
                                {:else}
                                  <span class="model-status">Not installed</span>
                                {/if}
                              </div>
                              <span class="field-description">{model.sizeLabel || model.fileName}</span>
                              {#if isDownloadingWhisperModel(model)}
                                <div
                                  class="model-progress"
                                  role="progressbar"
                                  aria-label={`Download progress for ${model.name}`}
                                  aria-valuemin="0"
                                  aria-valuemax="100"
                                  aria-valuenow={whisperDownloadProgress(model)}
                                >
                                  <div class="model-progress-header">
                                    <span>{whisperDownloadLabel(model)}</span>
                                    <span>{whisperDownloadProgress(model)}%</span>
                                  </div>
                                  <div class="update-progress-track">
                                    <div class="update-progress-fill" style={`width: ${whisperDownloadProgress(model)}%`}></div>
                                  </div>
                                </div>
                              {/if}
                            </div>
                            <div class="model-actions">
                              {#if model.installed}
                                <button
                                  class="btn"
                                  class:btn-primary={isSelectedWhisperModel(model)}
                                  type="button"
                                  aria-label={`Use model ${model.name}`}
                                  onclick={() => useWhisperModel(model)}
                                  disabled={isSelectedWhisperModel(model)}
                                >
                                  {isSelectedWhisperModel(model) ? 'Using' : 'Use'}
                                </button>
                                <button
                                  class="btn btn-danger"
                                  type="button"
                                  aria-label={`Delete model ${model.name}`}
                                  onclick={() => deleteWhisperModel(model)}
                                >
                                  Delete
                                </button>
                              {:else}
                                <button
                                  class="btn"
                                  type="button"
                                  aria-label={`Download model ${model.name}`}
                                  disabled={!model.url || isAnyWhisperDownloadActive()}
                                  onclick={() => downloadWhisperModel(model)}
                                >
                                  {isDownloadingWhisperModel(model) ? 'Downloading' : 'Download'}
                                </button>
                              {/if}
                            </div>
                          </div>
                        {/each}
                      </div>
                    {/if}

                    {#if whisperModelsError}
                      <span class="field-description update-error">{whisperModelsError}</span>
                    {/if}
                  </div>

                  <div class="field-row">
                    <span class="field-label">Search Hugging Face</span>
                    <div class="model-search-row">
                      <input
                        class="input"
                        type="text"
                        placeholder="Search compatible GGML Whisper models"
                        bind:value={whisperSearchQuery}
                        onkeydown={(event) => {
                          if (event.key === 'Enter') void searchWhisperModels();
                        }}
                      />
                      <button
                        class="btn"
                        type="button"
                        disabled={whisperSearchLoading || whisperSearchQuery.trim().length < 2}
                        onclick={searchWhisperModels}
                      >
                        {whisperSearchLoading ? 'Searching' : 'Search'}
                      </button>
                    </div>
                    {#if whisperSearchError}
                      <span class="field-description update-error">{whisperSearchError}</span>
                    {/if}
                    {#if whisperSearchResults.length > 0}
                      <div class="model-list compact">
                        {#each whisperSearchResults as model}
                          <div class="model-row">
                            <div class="model-main">
                              <div class="model-title-row">
                                <span class="model-name">{model.name}</span>
                                <span class="model-status">Hugging Face</span>
                              </div>
                              <span class="field-description">{model.fileName}</span>
                            </div>
                            <div class="model-actions">
                              <button
                                class="btn"
                                type="button"
                                aria-label={`Download model ${model.name}`}
                                disabled={!model.url || isAnyWhisperDownloadActive()}
                                onclick={() => downloadWhisperModel(model)}
                              >
                                {isDownloadingWhisperModel(model) ? 'Downloading' : 'Download'}
                              </button>
                            </div>
                          </div>
                        {/each}
                      </div>
                    {/if}
                  </div>

                  <div class="field-row">
                    <span class="field-label">Manual model path</span>
                    <input
                      class="input"
                      type="text"
                      placeholder={selectedWhisperModel?.path ?? 'Enter a GGML .bin model path'}
                      bind:value={txModel}
                    />
                    <span class="field-description">
                      Choose a downloaded model above, or paste an absolute GGML .bin path. A model must be selected to use Local Whisper.
                    </span>
                  </div>
                {:else}
                  <div class="field-row">
                    <span class="field-label">{txModelLabel}</span>
                    <input
                      class="input"
                      type="text"
                      placeholder={txDefaultModels[txProvider] ?? ''}
                      bind:value={txModel}
                    />
                    <span class="field-description">
                      {txModelDescription}
                    </span>
                  </div>
                {/if}

                {#if txProvider === 'deepgram'}
                  <div class="field-divider"></div>

                  <div class="toggle-row">
                    <div class="toggle-info">
                      <span class="toggle-label">Smart Format</span>
                      <span class="toggle-description">Format numbers, dates, currencies, and punctuation.</span>
                    </div>
                    <label class="toggle-switch">
                      <input type="checkbox" bind:checked={dgSmartFormat} />
                      <span class="toggle-track"></span>
                      <span class="toggle-thumb"></span>
                    </label>
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
                    <span class="field-label">Prompt</span>
                    <input
                      class="input"
                      type="text"
                      placeholder="e.g. The speaker discusses SwiftUI and Xcode"
                      bind:value={oaiPrompt}
                    />
                    <span class="field-description">Add context for names, terms, or jargon that may be misheard.</span>
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

              {/if}
            </div>
          </section>
        {/if}

        {#if activeSection === 'formatting'}
          <section class="settings-section" aria-label="Formatting settings">
            <div class="section-body">
              <div class="field-row">
                <span class="field-label">Provider</span>
                <div class="select-wrapper">
                  <select
                    class="select"
                    bind:value={fmtProvider}
                    aria-describedby={fmtProvider === 'apple' ? 'apple-formatting-warning' : undefined}
                  >
                    {#each fmtProviderOptions as p}
                      <option value={p.value}>{p.label}</option>
                    {/each}
                  </select>
                  <button class="select-toggle" aria-hidden="true" tabindex="-1" type="button">
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8">
                      <path d="M4 6L8 10L12 6" stroke-linecap="round" stroke-linejoin="round"/>
                    </svg>
                  </button>
                </div>
                {#if !hasFmtProvider}
                  <span class="field-description">Paste the raw transcription without cleanup.</span>
                {:else if fmtProvider === 'apple'}
                  <span id="apple-formatting-warning" class="field-description" role="note">
                    <strong class="update-error">Warning.</strong> Apple formatting may not follow your custom instructions reliably. Check the result before sending.
                  </span>
                {/if}
              </div>

              {#if hasFmtProvider}
                <div class="field-divider"></div>

                {#if fmtProviderRequiresApiKey}
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

                  <div class="field-row">
                    <span class="field-label">Model</span>
                    <input
                      class="input"
                      type="text"
                      placeholder={fmtDefaultModels[fmtProvider] ?? ''}
                      bind:value={fmtModel}
                    />
                    <span class="field-description">
                      Override the default model, or leave empty to use {fmtDefaultModels[fmtProvider] ?? 'none'}.
                    </span>
                  </div>

                  <div class="field-divider"></div>
                {/if}

                {#if fmtProvider === 'ollama'}
                  <div class="field-row">
                    <div class="local-models-header">
                      <div class="field-copy">
                        <span class="field-label">Ollama Models</span>
                        <span class="field-description">
                          Models are pulled into your local Ollama installation.
                        </span>
                      </div>
                      <button class="btn" type="button" onclick={loadOllamaModels}>Refresh</button>
                    </div>

                    {#if ollamaModelList && !ollamaModelList.serviceAvailable}
                      <div class="model-setup-state">
                        <div class="field-copy">
                          <span class="model-setup-title">Ollama is not running</span>
                          <span class="field-description">
                            Install Ollama, open it, then refresh to enable local model downloads.
                          </span>
                        </div>
                        <div class="model-actions">
                          <button class="btn btn-primary" type="button" onclick={openOllamaDownload}>
                            Install Ollama
                          </button>
                          <button class="btn" type="button" onclick={loadOllamaModels}>
                            Refresh
                          </button>
                        </div>
                      </div>
                    {/if}

                    {#if ollamaModelsLoading && !ollamaModelList}
                      <div class="history-empty-state">
                        <span>Loading models...</span>
                      </div>
                    {:else if ollamaModelList?.serviceAvailable && ollamaModelList.models.length === 0}
                      <div class="history-empty-state">
                        <span>No models installed. Search the Ollama library below to download one.</span>
                      </div>
                    {:else if ollamaModelList && ollamaModelList.models.length > 0}
                      <div class="model-list">
                        {#each ollamaModelList.models as model}
                          <div class="model-row" class:selected={isSelectedOllamaModel(model)}>
                            <div class="model-main">
                              <div class="model-title-row">
                                <span class="model-name">{model.name}</span>
                                {#if model.installed}
                                  <span class="model-status installed">Installed</span>
                                {:else}
                                  <span class="model-status">Not installed</span>
                                {/if}
                              </div>
                              <span class="field-description">{ollamaModelMeta(model) || model.id}</span>
                              {#if isDownloadingOllamaModel(model)}
                                <div
                                  class="model-progress"
                                  role="progressbar"
                                  aria-label={`Download progress for ${model.name}`}
                                  aria-valuemin="0"
                                  aria-valuemax="100"
                                  aria-valuenow={ollamaDownloadProgress(model)}
                                >
                                  <div class="model-progress-header">
                                    <span>{ollamaDownloadLabel(model)}</span>
                                    <span>{ollamaDownloadProgress(model)}%</span>
                                  </div>
                                  <div class="update-progress-track">
                                    <div class="update-progress-fill" style={`width: ${ollamaDownloadProgress(model)}%`}></div>
                                  </div>
                                </div>
                              {/if}
                            </div>
                            <div class="model-actions">
                              {#if model.installed}
                                <button
                                  class="btn"
                                  class:btn-primary={isSelectedOllamaModel(model)}
                                  type="button"
                                  aria-label={`Use model ${model.name}`}
                                  onclick={() => useOllamaModel(model)}
                                  disabled={isSelectedOllamaModel(model)}
                                >
                                  {isSelectedOllamaModel(model) ? 'Using' : 'Use'}
                                </button>
                                <button
                                  class="btn btn-danger"
                                  type="button"
                                  aria-label={`Delete model ${model.name}`}
                                  onclick={() => deleteOllamaModel(model)}
                                >
                                  Delete
                                </button>
                              {:else}
                                <button
                                  class="btn"
                                  type="button"
                                  aria-label={`Download model ${model.name}`}
                                  disabled={!ollamaModelList.serviceAvailable || isAnyOllamaDownloadActive()}
                                  onclick={() => pullOllamaModel(model)}
                                >
                                  {isDownloadingOllamaModel(model) ? 'Downloading' : 'Download'}
                                </button>
                              {/if}
                            </div>
                          </div>
                        {/each}
                      </div>
                    {/if}

                    {#if ollamaModelsError}
                      <span class="field-description update-error">{ollamaModelsError}</span>
                    {/if}
                  </div>

                  <div class="field-row">
                    <span class="field-label">Search Ollama Library</span>
                    <div class="model-search-row">
                      <input
                        class="input"
                        type="text"
                        placeholder="Search by model name or tag"
                        bind:value={ollamaSearchQuery}
                        oninput={() => {
                          ollamaSearchResults = [];
                          ollamaSearchError = '';
                          ollamaSearchedQuery = '';
                        }}
                        onkeydown={(event) => {
                          if (event.key === 'Enter') void searchOllamaModels();
                        }}
                      />
                      <button
                        class="btn"
                        type="button"
                        disabled={ollamaSearchLoading || ollamaSearchQuery.trim().length < 2}
                        onclick={searchOllamaModels}
                      >
                        {ollamaSearchLoading ? 'Searching' : 'Search'}
                      </button>
                    </div>
                    {#if ollamaSearchError}
                      <span class="field-description update-error">{ollamaSearchError}</span>
                    {:else if ollamaSearchedQuery && !ollamaSearchLoading && ollamaSearchResults.length === 0}
                      <span class="field-description">No models found for “{ollamaSearchedQuery}”.</span>
                    {/if}
                    {#if ollamaSearchResults.length > 0}
                      <div class="model-list compact">
                        {#each ollamaSearchResults as model}
                          <div class="model-row">
                            <div class="model-main">
                              <div class="model-title-row">
                                <span class="model-name">{model.name}</span>
                                <span class="model-status">{model.installed ? 'Installed' : 'Ollama Library'}</span>
                              </div>
                              <span class="field-description">{ollamaModelMeta(model) || model.id}</span>
                              {#if isDownloadingOllamaModel(model)}
                                <div
                                  class="model-progress"
                                  role="progressbar"
                                  aria-label={`Download progress for ${model.name}`}
                                  aria-valuemin="0"
                                  aria-valuemax="100"
                                  aria-valuenow={ollamaDownloadProgress(model)}
                                >
                                  <div class="model-progress-header">
                                    <span>{ollamaDownloadLabel(model)}</span>
                                    <span>{ollamaDownloadProgress(model)}%</span>
                                  </div>
                                  <div class="update-progress-track">
                                    <div class="update-progress-fill" style={`width: ${ollamaDownloadProgress(model)}%`}></div>
                                  </div>
                                </div>
                              {/if}
                            </div>
                            <div class="model-actions">
                              {#if model.installed}
                                <button
                                  class="btn"
                                  class:btn-primary={isSelectedOllamaModel(model)}
                                  type="button"
                                  aria-label={`Use model ${model.name}`}
                                  onclick={() => useOllamaModel(model)}
                                  disabled={isSelectedOllamaModel(model)}
                                >
                                  {isSelectedOllamaModel(model) ? 'Using' : 'Use'}
                                </button>
                              {:else}
                                <button
                                  class="btn"
                                  type="button"
                                  aria-label={`Download model ${model.name}`}
                                  disabled={!ollamaModelList?.serviceAvailable || isAnyOllamaDownloadActive()}
                                  onclick={() => pullOllamaModel(model)}
                                >
                                  {isDownloadingOllamaModel(model) ? 'Downloading' : 'Download'}
                                </button>
                              {/if}
                            </div>
                          </div>
                        {/each}
                      </div>
                    {/if}
                  </div>

                  <div class="field-row">
                    <span class="field-label">Manual model name</span>
                    <input
                      class="input"
                      type="text"
                      placeholder="Enter an installed model name"
                      bind:value={fmtModel}
                    />
                    <span class="field-description">
                      Choose an installed model above, or enter its name. A model must be selected to use Ollama formatting.
                    </span>
                  </div>

                  <div class="field-divider"></div>
                {/if}

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

                {#if fmtStyle === 'custom'}
                  <CustomInstructions bind:value={fmtCustomPrompt} />
                {/if}

                {#if fmtStyle !== 'custom'}
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
              {/if}
            </div>
          </section>
        {/if}

        {#if activeSection === 'history'}
          <section class="settings-section" aria-label="History settings">
            <div class="section-body">
              <div class="toggle-row">
                <div class="toggle-info">
                  <span class="toggle-label">Save transcription history</span>
                  <span class="toggle-description">Keep a local history of recent transcripts.</span>
                </div>
                <label class="toggle-switch">
                  <input type="checkbox" bind:checked={historyEnabled} />
                  <span class="toggle-track"></span>
                  <span class="toggle-thumb"></span>
                </label>
              </div>

              <div class="field-divider"></div>

              <div class="history-toolbar">
                <div class="field-copy">
                  <span class="field-label">Transcription history</span>
                  <span class="field-description">
                    {#if historyLoading}
                      Loading entries...
                    {:else if historyEntries.length === 1}
                      1 saved entry
                    {:else}
                      {historyEntries.length} saved entries
                    {/if}
                  </span>
                </div>
                <button
                  class="btn btn-danger"
                  onclick={clearHistoryEntries}
                  type="button"
                  disabled={historyLoading || historyEntries.length === 0}
                >
                  Clear
                </button>
              </div>

              {#if historyLoading}
                <div class="history-empty-state">
                  <span>Loading...</span>
                </div>
              {:else if historyEntries.length === 0}
                <div class="history-empty-state">
                  <span class="history-empty-title">No transcriptions yet</span>
                  <span class="field-description">Recent transcripts will appear here.</span>
                </div>
              {:else}
                <div class="settings-history-list" aria-label="Saved transcriptions">
                  {#each historyEntries as entry (entry.id)}
                    <article class="settings-history-entry">
                      <div class="history-entry-copy">
                        <div class="history-entry-text">{entry.text}</div>
                        <div class="history-entry-meta">
                          <span>{relativeTime(entry.timestamp)}</span>
                          <span class="history-badge">{providerLabel(entry.transcriptionProvider, entry.formattingProvider)}</span>
                          {#if entry.formattingStyle && entry.formattingStyle !== 'none'}
                            <span class="history-badge">{entry.formattingStyle}</span>
                          {/if}
                          {#if historyModelLabel(entry)}
                            <span class="history-badge">{historyModelLabel(entry)}</span>
                          {/if}
                        </div>
                        {#if hasHistoryDetails(entry)}
                          <details class="history-details">
                            <summary>Details</summary>
                            <div class="history-detail-grid">
                              {#if entry.rawText}
                                <div class="history-detail-block">
                                  <span class="history-detail-label">Raw</span>
                                  <pre>{entry.rawText}</pre>
                                </div>
                              {/if}
                              {#if entry.formattedText}
                                <div class="history-detail-block">
                                  <span class="history-detail-label">Formatted</span>
                                  <pre>{entry.formattedText}</pre>
                                </div>
                              {/if}
                              {#if entry.formattingInstruction}
                                <div class="history-detail-block">
                                  <span class="history-detail-label">{historyInstructionLabel(entry)}</span>
                                  <pre>{entry.formattingInstruction}</pre>
                                </div>
                              {/if}
                            </div>
                          </details>
                        {/if}
                      </div>
                      <div class="history-entry-actions">
                        <button
                          class="icon-button"
                          class:success={copiedHistoryId === entry.id}
                          onclick={() => copyHistoryEntry(entry)}
                          aria-label="Copy transcription"
                          type="button"
                        >
                          {#if copiedHistoryId === entry.id}
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                              <polyline points="3.5 8.5 6.5 11.5 12.5 5.5"/>
                            </svg>
                          {:else}
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                              <rect x="5" y="5" width="9" height="9" rx="1.5"/>
                              <path d="M3 11V3a1.5 1.5 0 0 1 1.5-1.5H11"/>
                            </svg>
                          {/if}
                        </button>
                        <button
                          class="icon-button danger"
                          onclick={() => deleteHistoryEntry(entry.id)}
                          aria-label="Delete transcription"
                          type="button"
                        >
                          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                            <path d="M2 4h12M5.333 4V2.667a1.333 1.333 0 0 1 1.334-1.334h2.666a1.333 1.333 0 0 1 1.334 1.334V4m2 0v9.333a1.333 1.333 0 0 1-1.334 1.334H4.667a1.333 1.333 0 0 1-1.334-1.334V4h9.334Z"/>
                          </svg>
                        </button>
                      </div>
                    </article>
                  {/each}
                </div>
              {/if}
            </div>
            {#if !historyEnabled}
              <div class="section-footer">
                Transcriptions will not be saved to disk.
              </div>
            {/if}
          </section>
        {/if}

        {#if activeSection === 'advanced'}
          <section class="settings-section" aria-label="Advanced settings">
            <div class="section-body">
              <div class="action-row">
                <div class="field-copy">
                  <span class="field-label">Software updates</span>
                  <span class="field-description">{updateMessage}</span>
                </div>
                {#if updateStatus === 'available'}
                  <button class="btn btn-primary" onclick={installUpdate} type="button">
                    Install Update
                  </button>
                {:else}
                  <button class="btn btn-secondary" onclick={checkForUpdates} type="button" disabled={updateBusy()}>
                    {updateStatus === 'checking' ? 'Checking...' : 'Check for Updates'}
                  </button>
                {/if}
              </div>

              {#if updateStatus === 'downloading' || updateStatus === 'ready'}
                <div class="update-progress" aria-label="Update progress">
                  <div class="update-progress-header">
                    <span>{updateStatus === 'ready' ? 'Installing' : 'Downloading'}</span>
                    <span>{updateProgressLabel()}</span>
                  </div>
                  <div class="update-progress-track">
                    <div class="update-progress-fill" style={`width: ${updateProgress()}%`}></div>
                  </div>
                </div>
              {/if}

              {#if updateStatus === 'available' && updateVersion}
                <div class="section-footer">
                  Version {updateVersion} will be verified before it is installed.
                </div>
              {:else if updateStatus === 'error'}
                <div class="section-footer update-error">
                  Yap did not install anything.
                </div>
              {/if}

              <div class="field-divider"></div>

              <div class="action-row">
                <div class="field-copy">
                  <span class="field-label">Local cache</span>
                  <span class="field-description">{cacheMessage}</span>
                </div>
                <button class="btn btn-secondary" onclick={cleanCache} type="button" disabled={cacheCleaning}>
                  {cacheCleaning ? 'Cleaning...' : 'Clean Cache'}
                </button>
              </div>

              <div class="field-divider"></div>

              <div class="action-row">
                <div class="field-copy">
                  <span class="field-label">Default settings</span>
                  <span class="field-description">Restore Yap’s default settings.</span>
                </div>
                <button class="btn btn-secondary" onclick={resetDefaults} type="button">
                  Reset Defaults
                </button>
              </div>
            </div>
          </section>
        {/if}
        </main>
      </div>
    </div>
  </div>
{/if}
