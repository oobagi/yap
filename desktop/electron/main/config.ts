import { app } from "electron";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

export type BackgroundAudioMode = "off" | "mute" | "pause";
export type AppearanceMode = "system" | "light" | "dark";
export type TranscriptionProvider =
  | "none"
  | "localwhisper"
  | "gemini"
  | "openai"
  | "deepgram"
  | "elevenlabs";
export type FormattingProvider =
  | "none"
  | "gemini"
  | "openai"
  | "anthropic"
  | "groq"
  | "apple"
  | "ollama";
export type FormattingStyle = "casual" | "formatted" | "professional" | "custom";

export interface AppConfig {
  hotkey: string;
  audioDevice: string;
  pressEnterAfterPaste: boolean;
  txProvider: TranscriptionProvider;
  txApiKey: string;
  txModel: string;
  fmtProvider: FormattingProvider;
  fmtApiKey: string;
  fmtModel: string;
  fmtStyle: FormattingStyle;
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

let cachedConfig: AppConfig = defaultConfig();

export function configDir(): string {
  const base = process.platform === "win32" ? app.getPath("appData") : join(homedir(), ".config");
  return join(base, "yap");
}

export function configPath(): string {
  return join(configDir(), "config.json");
}

export async function loadConfig(): Promise<AppConfig> {
  await mkdir(configDir(), { recursive: true });

  let config: AppConfig;
  try {
    const data = await readFile(configPath(), "utf8");
    config = normalizeConfig(JSON.parse(data));
  } catch {
    config = defaultConfig();
    await saveConfig(config);
  }

  cachedConfig = config;
  return config;
}

export async function saveConfig(config: AppConfig): Promise<void> {
  const normalized = normalizeConfig(config);
  await mkdir(configDir(), { recursive: true });
  await writeFile(configPath(), `${JSON.stringify(normalized, null, 2)}\n`, "utf8");
  cachedConfig = normalized;
}

export function getConfigSnapshot(): AppConfig {
  return cachedConfig;
}

function defaultConfig(): AppConfig {
  return {
    hotkey: process.platform === "win32" ? "capslock" : "fn",
    audioDevice: "",
    pressEnterAfterPaste: false,
    txProvider: process.platform === "win32" ? "localwhisper" : "none",
    txApiKey: "",
    txModel: "",
    fmtProvider: "none",
    fmtApiKey: "",
    fmtModel: "",
    fmtStyle: "formatted",
    fmtCustomPrompt: "",
    onboardingComplete: false,
    dgSmartFormat: true,
    dgKeywords: "",
    dgLanguage: "",
    oaiLanguage: "",
    oaiPrompt: "",
    geminiTemperature: 0,
    elLanguageCode: "",
    soundsEnabled: true,
    appearanceMode: "system",
    backgroundAudioMode: "mute",
    gradientEnabled: true,
    alwaysVisiblePill: true,
    historyEnabled: true,
    speechLocale: "",
  };
}

function normalizeConfig(input: Partial<AppConfig> | null | undefined): AppConfig {
  const defaults = defaultConfig();
  const config = { ...defaults, ...(input ?? {}) };
  const txProvider = normalizeTranscriptionProvider(config.txProvider, defaults.txProvider);
  const fmtProvider = normalizeFormattingProvider(config.fmtProvider, defaults.fmtProvider);

  return {
    ...config,
    txProvider,
    fmtProvider,
    hotkey: stringOrDefault(config.hotkey, defaults.hotkey),
    audioDevice: stringOrDefault(config.audioDevice, ""),
    txApiKey: stringOrDefault(config.txApiKey, ""),
    txModel: stringOrDefault(config.txModel, ""),
    fmtApiKey: stringOrDefault(config.fmtApiKey, ""),
    fmtModel: stringOrDefault(config.fmtModel, ""),
    fmtCustomPrompt: stringOrDefault(config.fmtCustomPrompt, ""),
    dgKeywords: stringOrDefault(config.dgKeywords, ""),
    dgLanguage: stringOrDefault(config.dgLanguage, ""),
    oaiLanguage: stringOrDefault(config.oaiLanguage, ""),
    oaiPrompt: stringOrDefault(config.oaiPrompt, ""),
    elLanguageCode: stringOrDefault(config.elLanguageCode, ""),
    speechLocale: stringOrDefault(config.speechLocale, ""),
    appearanceMode: isAppearanceMode(config.appearanceMode)
      ? config.appearanceMode
      : defaults.appearanceMode,
    backgroundAudioMode: isBackgroundAudioMode(config.backgroundAudioMode)
      ? config.backgroundAudioMode
      : defaults.backgroundAudioMode,
  };
}

function isTranscriptionProvider(value: unknown): value is TranscriptionProvider {
  return (
    value === "none" ||
    value === "localwhisper" ||
    value === "gemini" ||
    value === "openai" ||
    value === "deepgram" ||
    value === "elevenlabs"
  );
}

function normalizeTranscriptionProvider(
  value: unknown,
  fallback: TranscriptionProvider
): TranscriptionProvider {
  return isTranscriptionProvider(value) ? value : fallback;
}

function isFormattingProvider(value: unknown): value is FormattingProvider {
  return (
    value === "none" ||
    value === "gemini" ||
    value === "openai" ||
    value === "anthropic" ||
    value === "groq" ||
    value === "apple" ||
    value === "ollama"
  );
}

function normalizeFormattingProvider(
  value: unknown,
  fallback: FormattingProvider
): FormattingProvider {
  if (!isFormattingProvider(value)) return fallback;
  if (value === "apple" && process.platform !== "darwin") return fallback;
  return value;
}

function stringOrDefault(value: unknown, fallback: string): string {
  return typeof value === "string" ? value : fallback;
}

function isBackgroundAudioMode(value: unknown): value is BackgroundAudioMode {
  return value === "off" || value === "mute" || value === "pause";
}

function isAppearanceMode(value: unknown): value is AppearanceMode {
  return value === "system" || value === "light" || value === "dark";
}
