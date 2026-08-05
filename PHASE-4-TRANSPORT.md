# Phase 4 CLI transport foundation

The CLI no longer requires the deployed Archer server URL to be compiled into every request path.

Run setup with an explicit server:

```sh
archerdndsys setup --server https://example.test/api
```

Running `archerdndsys setup` without `--server` prompts for the URL and uses the legacy production URL when the prompt is left blank.

The selected URL is stored in `~/.archerdndsys/profiles.json` as a named `ServerProfile`. Existing installations without that file continue using the legacy default until setup is run again. Profiles can be listed or activated with:

```sh
archerdndsys profile list
archerdndsys profile use default
```

Local objects, tokens, and sync queues are scoped under a profile-specific data directory. Existing default-profile data is read from the legacy location for migration compatibility.

Authentication, sync calls, and TUI resource fetches use the shared `ApiClient` transport. Auto-login credentials are stored in the operating system keychain; legacy plaintext credentials are only used as a migration fallback. Sync requests use bounded concurrency, retry transient failures for idempotent methods, and leave failed session calls on disk for later retry.

Set `ARCHERDNDSYS_SYNC_CONCURRENCY` to tune sync worker count; the default is four.
