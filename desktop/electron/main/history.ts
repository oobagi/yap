import { clipboard } from "electron";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { configDir } from "./config";

export interface HistoryEntry {
  id: string;
  timestamp: string;
  text: string;
  rawText?: string | null;
  formattedText?: string | null;
  transcriptionProvider: string;
  transcriptionModel?: string | null;
  formattingProvider?: string | null;
  formattingModel?: string | null;
  formattingStyle?: string | null;
  formattingInstruction?: string | null;
}

const MAX_MENU_ENTRIES = 10;

function historyPath(): string {
  return join(configDir(), "history.json");
}

export async function loadHistory(): Promise<HistoryEntry[]> {
  try {
    const data = await readFile(historyPath(), "utf8");
    const parsed = JSON.parse(data);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export async function removeHistoryEntry(id: string): Promise<void> {
  const entries = await loadHistory();
  const filtered = entries.filter((entry) => entry.id !== id);
  if (filtered.length === entries.length) {
    throw new Error(`history entry not found: ${id}`);
  }
  await saveHistory(filtered);
}

export async function clearHistory(): Promise<void> {
  await saveHistory([]);
}

export async function copyHistoryEntryToClipboard(id: string): Promise<void> {
  const entry = (await loadHistory()).find((candidate) => candidate.id === id);
  if (entry) {
    clipboard.writeText(entry.text);
  }
}

export function menuEntries(entries: HistoryEntry[]): HistoryEntry[] {
  return entries.slice(0, MAX_MENU_ENTRIES);
}

export function historyMenuLabel(text: string): string {
  const singleLine = text.replace(/\n/g, " ").replace(/\r/g, "");
  if ([...singleLine].length <= 60) return singleLine;
  return `${[...singleLine].slice(0, 57).join("")}...`;
}

async function saveHistory(entries: HistoryEntry[]): Promise<void> {
  await writeFile(historyPath(), `${JSON.stringify(entries, null, 2)}\n`, "utf8");
}
