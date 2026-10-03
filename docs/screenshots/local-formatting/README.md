# Local formatting screenshots

Captured from the real Yap settings components at `/settings.html`, in headless Chrome at 1440 × 1000 CSS pixels (scale 1), on October 2, 2026.

Config, model availability, model sizes, and dictation history are synthetic fixtures. These images verify rendering only, not model inference or native persistence. They show the local feature implementation based on `8a079b7`; subsequent changes fix error handling without changing the pictured states.

- `01-apple-custom-instructions-fixture-desktop.png`: Apple on-device formatting with a custom instruction.
- `02-ollama-models-fixture-desktop.png`: Ollama model management, with example installed and available models.
- `03-dictation-history-fixture-desktop.png`: expanded history showing original text, formatted output, and the instruction used.

The capture used a fresh isolated browser and an in-memory preload bridge fixture, with external HTTP traffic and API mutations blocked. No personal config, API keys, or actual dictation history was used.
