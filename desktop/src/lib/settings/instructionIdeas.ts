export interface InstructionIdea {
  id: string;
  label: string;
  instruction: string;
}

interface InstructionCategory {
  id: string;
  label: string;
  ideas: InstructionIdea[];
}

export const instructionCategories: InstructionCategory[] = [
  {
    id: 'cleanup',
    label: 'Cleanup',
    ideas: [
      {
        id: 'filler',
        label: 'Remove filler words',
        instruction: 'Remove filler sounds and phrases such as "um", "uh", "like", and "you know" when they add no meaning. Keep them when they are part of the actual message.',
      },
      {
        id: 'repetition',
        label: 'Remove repetition',
        instruction: 'Remove accidental repeated words and repeated ideas, while keeping intentional emphasis.',
      },
      {
        id: 'corrections',
        label: 'Follow my corrections',
        instruction: 'When I correct myself mid-sentence, keep only the corrected version and remove the abandoned wording and correction phrases.',
      },
      {
        id: 'concise',
        label: 'Make it concise',
        instruction: 'Tighten wordy sentences without losing facts, requests, qualifications, or the original meaning.',
      },
      {
        id: 'wording',
        label: 'Keep my wording',
        instruction: 'Keep my vocabulary, phrasing, and sentence order. Only clean up filler words, punctuation, capitalization, and obvious grammar mistakes.',
      },
    ],
  },
  {
    id: 'structure',
    label: 'Structure',
    ideas: [
      {
        id: 'bullets',
        label: 'Bullet points',
        instruction: 'When I list multiple related items, put each item on its own line starting with "- ". Keep other text as prose.',
      },
      {
        id: 'steps',
        label: 'Numbered steps',
        instruction: 'Format sequences of steps as a numbered list using "1.", "2.", and so on. Preserve the order I describe.',
      },
      {
        id: 'paragraphs',
        label: 'Short paragraphs',
        instruction: 'Break longer dictation into short paragraphs, starting a new paragraph when the topic changes.',
      },
      {
        id: 'headings',
        label: 'Add headings',
        instruction: 'For longer dictation covering several topics, group related points under short, descriptive headings. Skip headings for short messages.',
      },
      {
        id: 'checklist',
        label: 'Task checklists',
        instruction: 'When I dictate a to-do list, format each task on a separate line beginning with "- [ ] ". Keep only tasks I actually mention.',
      },
    ],
  },
  {
    id: 'tone',
    label: 'Tone',
    ideas: [
      {
        id: 'natural',
        label: 'Sound like me',
        instruction: 'Keep my natural voice, contractions, and casual expressions. Avoid making the text sound formal or generic.',
      },
      {
        id: 'professional',
        label: 'Professional',
        instruction: 'Use clear, professional language suitable for work. Stay natural and avoid corporate jargon or overly formal phrasing.',
      },
      {
        id: 'direct',
        label: 'Get to the point',
        instruction: 'Lead with the main point or request, then give supporting details. Preserve uncertainty and qualifications that affect the meaning.',
      },
      {
        id: 'plain',
        label: 'Plain language',
        instruction: 'Use everyday words and straightforward sentences. Keep technical terms when they are needed for accuracy.',
      },
      {
        id: 'lowercase',
        label: 'All lowercase',
        instruction: 'Write ordinary prose in lowercase for a casual texting style. Preserve capitalization in code, case-sensitive identifiers, and URLs.',
      },
    ],
  },
  {
    id: 'work',
    label: 'Work',
    ideas: [
      {
        id: 'email',
        label: 'Email drafts',
        instruction: 'When I dictate an email, organize it into readable paragraphs with a clear purpose and request. Keep any greeting or sign-off I dictate, but do not invent names, a subject line, or a signature.',
      },
      {
        id: 'meeting',
        label: 'Meeting notes',
        instruction: 'When I dictate meeting notes, group them under Discussion, Decisions, and Next steps. Omit empty sections and include only details I mention.',
      },
      {
        id: 'actions',
        label: 'Action items',
        instruction: 'When I describe action items, put each action on its own bullet. Include an owner or deadline only when I explicitly mention one.',
      },
      {
        id: 'status',
        label: 'Status updates',
        instruction: 'When I dictate a project update, organize it under Progress, Next steps, and Blockers. Omit sections with no information and do not invent progress or commitments.',
      },
      {
        id: 'chat',
        label: 'Quick messages',
        instruction: 'For chat messages, keep the result brief and conversational, with the main point first. Do not add greetings, sign-offs, or extra pleasantries I did not dictate.',
      },
    ],
  },
  {
    id: 'technical',
    label: 'Technical',
    ideas: [
      {
        id: 'coding',
        label: 'Coding prompts',
        instruction: 'When I dictate a request for a coding assistant, organize it into the requested change, relevant context, and constraints. Preserve all requirements. Do not solve the task or add implementation details I did not mention.',
      },
      {
        id: 'bugs',
        label: 'Bug reports',
        instruction: 'When I describe a bug, organize the report into Steps to reproduce, Expected behavior, and Actual behavior. Include only information I provide and omit empty sections.',
      },
      {
        id: 'terms',
        label: 'Keep technical terms',
        instruction: 'Preserve technical terms, product names, file paths, and identifiers. Do not replace them with simpler words or guess at unfamiliar names.',
      },
      {
        id: 'code',
        label: 'Inline code',
        instruction: 'Wrap clearly identified code identifiers, commands, and file paths in single backticks. Preserve their spelling and case, and do not invent code.',
      },
      {
        id: 'plain-text',
        label: 'Plain text only',
        instruction: 'Use plain text without Markdown headings, bold, italics, backticks, or checkbox syntax. Separate paragraphs with blank lines.',
      },
    ],
  },
];
