<script lang="ts">
  /**
   * Transcription history window for the Yap Tauri app.
   *
   * Shows a scrollable list of past transcriptions with relative timestamps,
   * provider labels, copy/delete actions, and a clear-all button with
   * confirmation dialog.
   */

  import './history.css';
  import { invoke } from '@tauri-apps/api/core';

  // ── Types ─────────────────────────────────────────────────────────────

  interface HistoryEntry {
    id: string;
    timestamp: string; // ISO-8601
    text: string;
    transcriptionProvider: string;
    formattingProvider: string | null;
    formattingStyle: string | null;
  }

  // ── Provider Display Names ────────────────────────────────────────────

  const providerLabels: Record<string, string> = {
    none: 'On-device',
    gemini: 'Gemini',
    openai: 'OpenAI',
    deepgram: 'Deepgram',
    elevenlabs: 'ElevenLabs',
    anthropic: 'Anthropic',
    groq: 'Groq',
  };

  function providerLabel(tx: string, fmt: string | null): string {
    const txLabel = providerLabels[tx] ?? tx;
    if (!fmt || fmt === 'none') return txLabel;
    const fmtLabel = providerLabels[fmt] ?? fmt;
    if (txLabel === fmtLabel) return txLabel;
    return `${txLabel} + ${fmtLabel}`;
  }

  // ── Relative Time ─────────────────────────────────────────────────────

  function relativeTime(isoString: string): string {
    const now = Date.now();
    const then = new Date(isoString).getTime();
    const diffMs = now - then;
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHr = Math.floor(diffMin / 60);
    const diffDay = Math.floor(diffHr / 24);

    if (diffSec < 10) return 'just now';
    if (diffSec < 60) return `${diffSec}s ago`;
    if (diffMin < 60) return `${diffMin}m ago`;
    if (diffHr < 24) return `${diffHr}h ago`;
    if (diffDay === 1) return 'Yesterday';
    if (diffDay < 7) return `${diffDay}d ago`;
    if (diffDay < 30) return `${Math.floor(diffDay / 7)}w ago`;
    return new Date(isoString).toLocaleDateString();
  }

  // ── Truncate Text ─────────────────────────────────────────────────────

  function truncate(text: string, maxLen: number): string {
    if (text.length <= maxLen) return text;
    return text.slice(0, maxLen).trimEnd() + '...';
  }

  // ── State ─────────────────────────────────────────────────────────────

  let loading = $state(true);
  let entries = $state<HistoryEntry[]>([]);
  let copiedId = $state<string | null>(null);
  let showConfirm = $state(false);
  let copyTimeouts = new Map<string, ReturnType<typeof setTimeout>>();

  // ── Load History ──────────────────────────────────────────────────────

  async function loadHistory() {
    try {
      entries = await invoke<HistoryEntry[]>('get_history');
    } catch (e) {
      console.error('Failed to load history:', e);
      entries = [];
    }
    loading = false;
  }

  // ── Copy Entry ────────────────────────────────────────────────────────

  async function copyEntry(entry: HistoryEntry) {
    try {
      await navigator.clipboard.writeText(entry.text);

      // Clear any existing timeout for this id
      const existing = copyTimeouts.get(entry.id);
      if (existing) clearTimeout(existing);

      copiedId = entry.id;
      const timeout = setTimeout(() => {
        if (copiedId === entry.id) copiedId = null;
      }, 1500);
      copyTimeouts.set(entry.id, timeout);
    } catch (e) {
      console.error('Failed to copy:', e);
    }
  }

  // ── Delete Entry ──────────────────────────────────────────────────────

  async function deleteEntry(id: string) {
    try {
      await invoke('remove_history_entry', { id });
      entries = entries.filter((e) => e.id !== id);
    } catch (e) {
      console.error('Failed to delete entry:', e);
    }
  }

  // ── Clear History ─────────────────────────────────────────────────────

  async function clearHistory() {
    try {
      await invoke('clear_history');
      entries = [];
      showConfirm = false;
    } catch (e) {
      console.error('Failed to clear history:', e);
    }
  }

  // ── Close Window ──────────────────────────────────────────────────────

  async function closeWindow() {
    await invoke('hide_app_window', { label: 'history' });
  }

  // ── Keyboard ──────────────────────────────────────────────────────────

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (showConfirm) {
        showConfirm = false;
      } else {
        closeWindow();
      }
    }
  }

  // ── Init ──────────────────────────────────────────────────────────────

  loadHistory();
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="history-container">
  <!-- Header -->
  <div class="history-header">
    <span class="history-title">Transcription History</span>
    {#if entries.length > 0}
      <button class="btn-clear" onclick={() => { showConfirm = true; }} type="button">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M2 4h12M5.333 4V2.667a1.333 1.333 0 0 1 1.334-1.334h2.666a1.333 1.333 0 0 1 1.334 1.334V4m2 0v9.333a1.333 1.333 0 0 1-1.334 1.334H4.667a1.333 1.333 0 0 1-1.334-1.334V4h9.334Z"/>
        </svg>
        Clear All
      </button>
    {/if}
  </div>

  <!-- Content -->
  {#if loading}
    <div class="history-empty">
      <span class="empty-text">Loading...</span>
    </div>
  {:else if entries.length === 0}
    <!-- Empty State -->
    <div class="history-empty">
      <div class="empty-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
          <rect x="7" y="2" width="10" height="15" rx="5"/>
          <path d="M3 12c0 5 4 9 9 9s9-4 9-9"/>
          <line x1="12" y1="21" x2="12" y2="24"/>
          <line x1="8" y1="24" x2="16" y2="24"/>
        </svg>
      </div>
      <span class="empty-text">No transcriptions yet</span>
      <span class="empty-hint">Your transcription history will appear here</span>
    </div>
  {:else}
    <!-- Entry List -->
    <div class="history-list">
      {#each entries as entry (entry.id)}
        <div class="history-entry">
          <div class="entry-content">
            <div class="entry-text">{truncate(entry.text, 100)}</div>
            <div class="entry-meta">
              <span class="entry-time">{relativeTime(entry.timestamp)}</span>
              <span class="entry-provider">
                {providerLabel(entry.transcriptionProvider, entry.formattingProvider)}
              </span>
              {#if entry.formattingStyle && entry.formattingStyle !== 'none'}
                <span class="entry-provider" style="background: rgba(76, 175, 124, 0.12); color: #4caf7c;">
                  {entry.formattingStyle}
                </span>
              {/if}
            </div>
          </div>
          <div class="entry-actions">
            <!-- Copy Button -->
            <button
              class="btn-icon"
              class:copied={copiedId === entry.id}
              onclick={() => copyEntry(entry)}
              aria-label="Copy transcription"
              type="button"
            >
              {#if copiedId === entry.id}
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
            <!-- Delete Button -->
            <button
              class="btn-icon btn-delete"
              onclick={() => deleteEntry(entry.id)}
              aria-label="Delete entry"
              type="button"
            >
              <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">
                <path d="M2 4h12M5.333 4V2.667a1.333 1.333 0 0 1 1.334-1.334h2.666a1.333 1.333 0 0 1 1.334 1.334V4m2 0v9.333a1.333 1.333 0 0 1-1.334 1.334H4.667a1.333 1.333 0 0 1-1.334-1.334V4h9.334Z"/>
              </svg>
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Footer -->
  <div class="history-footer">
    <span>
      {#if entries.length === 0}
        No entries
      {:else if entries.length === 1}
        1 entry
      {:else}
        {entries.length} entries
      {/if}
    </span>
    <span style="font-size: 10px; color: var(--history-text-muted); opacity: 0.5;">
      Esc to close
    </span>
  </div>
</div>

<!-- ── Confirm Clear Dialog ──────────────────────────────────────────── -->
{#if showConfirm}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="confirm-overlay" onclick={() => { showConfirm = false; }}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="confirm-dialog" onclick={(e) => e.stopPropagation()}>
      <div class="confirm-title">Clear all history?</div>
      <div class="confirm-message">
        This will permanently delete all {entries.length} transcription
        {entries.length === 1 ? 'entry' : 'entries'}. This action cannot be undone.
      </div>
      <div class="confirm-actions">
        <button class="btn btn-cancel" onclick={() => { showConfirm = false; }} type="button">
          Cancel
        </button>
        <button class="btn btn-confirm-danger" onclick={clearHistory} type="button">
          Clear All
        </button>
      </div>
    </div>
  </div>
{/if}
