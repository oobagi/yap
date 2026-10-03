export type SectionId = 'general' | 'transcription' | 'formatting' | 'history' | 'advanced';
export type UpdateStatus = 'idle' | 'checking' | 'available' | 'upToDate' | 'downloading' | 'ready' | 'error';
export type BackgroundAudioMode = 'off' | 'mute' | 'pause';
export type AppearanceMode = 'system' | 'light' | 'dark';

export interface AppConfig {
  hotkey: string;
  audioDevice: string;
  pressEnterAfterPaste: boolean;
  txProvider: string;
  txApiKey: string;
  txModel: string;
  fmtProvider: string;
  fmtApiKey: string;
  fmtModel: string;
  fmtStyle: string;
  fmtCustomPrompt: string;
  onboardingComplete: boolean;
  dgSmartFormat: boolean;
  dgKeywords: string;
  dgLanguage: string;
  oaiLanguage: string;
  oaiPrompt: string;
  geminiTemperature: number;
  elLanguageCode: string;
  soundsEnabled: boolean;
  appearanceMode: AppearanceMode;
  backgroundAudioMode: BackgroundAudioMode;
  gradientEnabled: boolean;
  alwaysVisiblePill: boolean;
  historyEnabled: boolean;
  speechLocale: string;
}

export interface HistoryEntry {
  id: string;
  timestamp: string;
  text: string;
  rawText?: string | null;
  formattedText?: string | null;
  transcriptionProvider: string;
  transcriptionModel?: string | null;
  formattingProvider: string | null;
  formattingModel?: string | null;
  formattingStyle: string | null;
  formattingInstruction?: string | null;
}

export interface WhisperModelSummary {
  id: string;
  name: string;
  fileName: string;
  source: 'curated' | 'huggingface' | 'installed';
  url?: string;
  sizeBytes?: number;
  sizeLabel?: string;
  speedHint?: string;
  accuracyHint?: string;
  installed: boolean;
  path?: string;
}

export interface WhisperModelList {
  cacheDir: string;
  recommendedId: string;
  models: WhisperModelSummary[];
}

export interface WhisperDownloadEvent {
  id: string;
  fileName: string;
  status: 'started' | 'progress' | 'finished' | 'error';
  transferred?: number;
  total?: number;
  percent?: number;
  error?: string;
}

export interface OllamaModelSummary {
  id: string;
  name: string;
  source: 'curated' | 'library' | 'installed';
  sizeBytes?: number;
  sizeLabel?: string;
  speedHint?: string;
  qualityHint?: string;
  installed: boolean;
}

export interface OllamaModelList {
  recommendedId: string;
  serviceAvailable: boolean;
  serviceError?: string;
  models: OllamaModelSummary[];
}

export interface OllamaDownloadEvent {
  id: string;
  model: string;
  status: 'started' | 'progress' | 'finished' | 'error';
  message?: string;
  transferred?: number;
  total?: number;
  percent?: number;
  error?: string;
}

interface ProviderOption {
  value: string;
  label: string;
  disabled?: boolean;
}

export function transcriptionProviders(isWindows: boolean): ProviderOption[] {
  return [
    { value: 'none', label: isWindows ? 'On-device (macOS 26+)' : 'On-device (macOS 26+)', disabled: isWindows },
    { value: 'localwhisper', label: 'Local Whisper' },
    { value: 'gemini', label: 'Gemini' },
    { value: 'openai', label: 'OpenAI' },
    { value: 'deepgram', label: 'Deepgram' },
    { value: 'elevenlabs', label: 'ElevenLabs' },
  ];
}

export function transcriptionProviderRequiresApiKey(provider: string): boolean {
  return ['gemini', 'openai', 'deepgram', 'elevenlabs'].includes(provider);
}

export function formattingProviders(isMac: boolean): ProviderOption[] {
  return [
    { value: 'none', label: 'None' },
    ...(isMac ? [{ value: 'apple', label: 'Apple On-device' }] : []),
    { value: 'ollama', label: 'Ollama' },
    { value: 'gemini', label: 'Gemini' },
    { value: 'openai', label: 'OpenAI' },
    { value: 'anthropic', label: 'Anthropic' },
    { value: 'groq', label: 'Groq' },
  ];
}

export function formattingProviderRequiresApiKey(provider: string): boolean {
  return ['gemini', 'openai', 'anthropic', 'groq'].includes(provider);
}

export const txDefaultModels: Record<string, string> = {
  none: '',
  localwhisper: 'large-v3-turbo-q5_0',
  gemini: 'gemini-2.5-flash',
  openai: 'gpt-4o-transcribe',
  deepgram: 'nova-3',
  elevenlabs: 'scribe_v1',
};

export const fmtDefaultModels: Record<string, string> = {
  none: '',
  apple: '',
  ollama: 'qwen3.5:4b',
  gemini: 'gemini-2.5-flash',
  openai: 'gpt-4o-mini',
  anthropic: 'claude-haiku-4-5-20251001',
  groq: 'llama-3.3-70b-versatile',
};

export const languageOptions = [
  { value: 'auto', label: 'Auto-detect', providerCode: '', speechLocale: '' },
  { value: 'en', label: 'English', providerCode: 'en', speechLocale: 'en-US' },
  { value: 'es', label: 'Spanish', providerCode: 'es', speechLocale: 'es-ES' },
  { value: 'fr', label: 'French', providerCode: 'fr', speechLocale: 'fr-FR' },
  { value: 'de', label: 'German', providerCode: 'de', speechLocale: 'de-DE' },
  { value: 'it', label: 'Italian', providerCode: 'it', speechLocale: 'it-IT' },
  { value: 'pt', label: 'Portuguese', providerCode: 'pt', speechLocale: 'pt-PT' },
  { value: 'ja', label: 'Japanese', providerCode: 'ja', speechLocale: 'ja-JP' },
  { value: 'ko', label: 'Korean', providerCode: 'ko', speechLocale: 'ko-KR' },
  { value: 'zh', label: 'Chinese', providerCode: 'zh', speechLocale: 'zh-CN' },
];

export const styleData: Record<string, { label: string; description: string; example: string }> = {
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
  custom: {
    label: 'Custom',
    description: 'Your instructions for rewriting dictation',
    example: 'Yeah, I was thinking we could try that new place on Friday, if your schedule allows.',
  },
};

export const styleExampleInput = 'yeah i was thinking we could try that new place on friday if youre free';
export const modifierOrder = ['cmd', 'ctrl', 'option', 'shift', 'fn'];

export const providerLabels: Record<string, string> = {
  none: 'On-device (macOS 26+)',
  localwhisper: 'Local Whisper',
  gemini: 'Gemini',
  openai: 'OpenAI',
  deepgram: 'Deepgram',
  elevenlabs: 'ElevenLabs',
  anthropic: 'Anthropic',
  groq: 'Groq',
  apple: 'Apple On-device',
  ollama: 'Ollama',
};

export const settingsSections: Array<{ id: SectionId; label: string; description: string }> = [
  { id: 'general', label: 'General', description: 'Hotkey, microphone, and app behavior' },
  { id: 'transcription', label: 'Transcription', description: 'Transcription provider and model' },
  { id: 'formatting', label: 'Formatting', description: 'Formatting provider and model' },
  { id: 'history', label: 'History', description: 'Recent transcript history' },
  { id: 'advanced', label: 'Advanced', description: 'Updates and defaults' },
];

export const backgroundAudioModes: Array<{ value: BackgroundAudioMode; label: string }> = [
  { value: 'off', label: 'Off' },
  { value: 'mute', label: 'Mute' },
  { value: 'pause', label: 'Pause' },
];

export const appearanceModes: Array<{ value: AppearanceMode; label: string }> = [
  { value: 'system', label: 'System' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];
