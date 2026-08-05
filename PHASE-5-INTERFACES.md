# Phase 5 — CLI/TUI dual interface

Phase 5 establishes one client with two deliberate interaction modes:

- `run --interface tui` opens the interactive terminal UI.
- `run --interface cli <command>` executes one command and exits.
- `run` remains backward-compatible and opens the TUI.
- `run <command>` remains convenient and executes through the CLI path.
- `run --interface auto` makes the selection rule explicit: no command means
  TUI; a command means CLI.

The CLI and TUI share the same profile-scoped cache, authentication, transport,
and object operations. This keeps the two interfaces behaviorally aligned and
avoids a second API client implementation.

Examples:

```text
archerdndsys run
archerdndsys run --interface tui
archerdndsys run --interface cli list
archerdndsys run --interface cli view
archerdndsys run sync
```

The TUI path checks authentication and prepares its in-memory cache before
entering raw terminal mode. Terminal cleanup is performed even when the event
loop returns an input or drawing error. The CLI path is suitable for scripts,
low-resource machines, and users who prefer line-oriented interaction.

Resource fetches use a profile-scoped cache manifest. A login starts a new
client session; the first fetch of each resource in that session validates and
stores the response, and later reads reuse the local JSON. Cached files remain
available for inspection across sessions but are revalidated once after login.
Successful queued writes are removed from their session queue. Automatic
transport retries are limited to idempotent methods; POST requests require the
server or caller to provide an explicit idempotency strategy.

Next Phase 5 work should add non-interactive object arguments/flags, stable exit
codes and machine-readable output, then expand TUI editing and onboarding
flows. Handwriting OCR belongs behind a separate import workflow so the base
CLI/TUI remains lightweight for enthusiast hardware.
