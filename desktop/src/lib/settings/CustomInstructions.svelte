<script lang="ts">
  import { instructionCategories, type InstructionIdea } from './instructionIdeas';

  let { value = $bindable('') }: { value: string } = $props();

  let categoryId = $state(instructionCategories[0].id);
  let announcement = $state('');
  let category = $derived(instructionCategories.find((item) => item.id === categoryId) ?? instructionCategories[0]);
  let normalizedValue = $derived(value.replace(/\s+/g, ' ').trim());

  function isAdded(idea: InstructionIdea): boolean {
    return normalizedValue.includes(idea.instruction);
  }

  function addIdea(idea: InstructionIdea) {
    if (isAdded(idea)) return;

    const separator = value.length === 0 || value.endsWith('\n') ? '' : '\n';
    value = `${value}${separator}${idea.instruction}`;
    announcement = `Added ${idea.label} to custom instructions.`;
  }
</script>

<div class="field-row instruction-ideas">
  <div class="ideas-header">
    <div class="field-copy">
      <span id="instruction-ideas-label" class="field-label">Instruction ideas</span>
      <span class="field-description">Click an idea to add it below.</span>
    </div>
    <div class="select-wrapper idea-category">
      <select class="select" aria-label="Instruction category" bind:value={categoryId}>
        {#each instructionCategories as item (item.id)}
          <option value={item.id}>{item.label}</option>
        {/each}
      </select>
      <span class="select-toggle" aria-hidden="true">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8">
          <path d="M4 6L8 10L12 6" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </span>
    </div>
  </div>

  <div class="idea-pills" role="group" aria-labelledby="instruction-ideas-label">
    {#each category.ideas as idea (idea.id)}
      {@const added = isAdded(idea)}
      <button
        class="btn idea-pill"
        class:added
        type="button"
        aria-disabled={added}
        aria-label={`${added ? 'Already added' : 'Add'}: ${idea.label}. ${idea.instruction}`}
        title={idea.instruction}
        onclick={() => addIdea(idea)}
      >
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">
          {#if added}
            <path d="m3 8 3 3 7-7" stroke-linecap="round" stroke-linejoin="round" />
          {:else}
            <path d="M8 3v10M3 8h10" stroke-linecap="round" />
          {/if}
        </svg>
        {idea.label}
      </button>
    {/each}
  </div>
  <span class="sr-only" role="status">{announcement}</span>
</div>

<div class="field-row">
  <label class="field-label" for="custom-instructions">Custom instructions</label>
  <textarea
    id="custom-instructions"
    class="input textarea"
    placeholder="Add an idea above or write your own instructions…"
    bind:value
    rows="5"
    aria-describedby="custom-instructions-help"
  ></textarea>
  <span id="custom-instructions-help" class="field-description">
    Edit these instructions to make them your own.
  </span>
</div>

<style>
  .instruction-ideas {
    gap: 12px;
  }

  .ideas-header {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .idea-category {
    flex: 0 0 132px;
  }

  .idea-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .idea-pill {
    gap: 7px;
    padding: 0 10px;
  }

  .idea-pill svg {
    width: 13px;
    height: 13px;
    flex: 0 0 13px;
    color: var(--settings-text-muted);
  }

  .idea-pill.added {
    border-color: var(--settings-primary-border);
    background: var(--settings-primary-soft);
    cursor: default;
  }

  .idea-pill.added svg {
    color: var(--settings-primary-text);
  }

  .idea-pill:focus-visible {
    outline: 2px solid var(--settings-primary-text);
    outline-offset: 2px;
  }
</style>
