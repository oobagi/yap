# Local model settings screenshots

Captured from the real Yap settings components at `/settings.html`, using headless Chrome at 1440 × 1000 CSS pixels (scale 1).

Capture time: 2026-10-03T07:39:08.930Z. Source: local changes based on `e3103fdf5047562c7732b43e6f6e3f4dfeb12961` (dirty working tree before the ship commit).

- `01-general-dark-fixture-desktop.png`: green controls in dark appearance.
- `02-apple-warning-light-fixture-desktop.png`: Apple formatting retained with a red warning label.
- `03-ollama-search-dark-fixture-desktop.png`: installed model and an empty search result, without recommendations or an invented download.
- `04-whisper-empty-light-fixture-desktop.png`: empty installed Whisper list with explicit model selection.

Config, models, and search responses are synthetic fixtures. These images verify rendering only; catalog searches and native behavior were checked separately. The isolated browser used an in-memory preload bridge, blocked external traffic and mutations, and contained no personal config, API keys, or dictation history. The existing dev-server favicon URL was served from the real `desktop/static/favicon.png` asset in the harness.

Local capture harness (ignored): `output/playwright/ship-local-models/capture.mjs`. With `pnpm run dev` running in `desktop`, rerun from the repository root using `node output/playwright/ship-local-models/capture.mjs`.
