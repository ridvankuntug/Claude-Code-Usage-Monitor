# Claude Desktop History Fallback Maintenance

This note is for carrying the local Claude Desktop usage history fallback forward after upstream updates.

## Goal

Keep this behavior available even after pulling new versions from `CodeZeno/Claude-Code-Usage-Monitor`:

- Run upstream's Claude polling first, unchanged. Since v2.x upstream tries, in order, the CLI credentials (`~/.claude/.credentials.json`), the Claude desktop app's own DPAPI/OSCrypt-encrypted OAuth token cache, and WSL credentials, then refreshes the token and calls the usage endpoint (with its messages-header fallback).
- Only if that chain ends in a credential or login failure (`NoCredentials`, `TokenExpired`, `AuthRequired`, HTTP 401/403), read Claude Desktop's local `plan-usage-history.json`.
- Transient failures (`NetworkError`, `RequestFailed`, `UnexpectedResponse`, other HTTP statuses such as 429/5xx) never use the fallback, so upstream keeps the last good reading as stale and retries sooner.
- If no usable history file is found, return the original upstream error unchanged, so the UI keeps showing upstream's states (sign in again, no login found, ...).
- The fallback reads only `plan-usage-history.json`. It does not touch tokens, cookies, Local Storage, or `Network\Cookies` itself; upstream's own desktop token reader is left as is.
- History files are searched, first readable and parseable file wins, in this order:
  - `%LOCALAPPDATA%\Claude-Data\claude_<publisher>\plan-usage-history.json` (migrated desktop data; `claude_pzs8sxrjxfjjc` first)
  - `%APPDATA%\Claude\plan-usage-history.json`
  - `%LOCALAPPDATA%\Packages\Claude_<publisher>\LocalCache\Roaming\Claude\plan-usage-history.json` (MSIX)
  - `%LOCALAPPDATA%\Claude\plan-usage-history.json` (fork's legacy candidate)

  The first three come from upstream's `claude_desktop::data_directories_in()`, so new upstream desktop locations are picked up automatically. Duplicates are dropped while keeping upstream's order.
- Pick the sample with the highest `t` (Unix milliseconds; samples without `t` rank oldest).
- Map `u.fh` to `session.percentage` and `u.sd` to `weekly.percentage`; both must be present.
- Clamp both percentages to `0..100`, mark both windows `available`, and leave both `resets_at` values as `None`.

### Scope and limits

- Upstream always polls Claude through the accounts layer (`AccountSettings::get(Claude)` is always `Some`). The default profile goes to `claude::poll_account(path)` when a readable `~/.claude/.credentials.json` exists, otherwise to `claude::poll_claude_code()`. Both are wrapped.
- `poll_account` uses the fallback only for the default CLI credentials path, using upstream's `desktop_fallback_allowed()` rule for the desktop token. Custom account profiles stay pinned and keep upstream behavior.
- Samples carry an `org` id, but upstream profiles do not know their organization id without an extra API call, so samples are not matched to profiles or organizations. If the CLI and the desktop app are signed in to different organizations, the newest sample may belong to the desktop app's organization.
- Like upstream's desktop token lookup, a non-empty `CLAUDE_CONFIG_DIR` is treated as an explicit account selection: the fallback is skipped and the upstream error is returned.
- Desktop history does not include reset times, so the countdown is empty while the fallback is in use.
- The newest sample is used as is, however old it is, and is not marked stale.
- While the fallback supplies usage, upstream's sign-in notification and auth pause for that failure do not happen, so upstream keeps trying its own credential refresh on each poll.
- `plan-usage-history.json` is not part of the account's credential-watch signature. If a source was paused by an auth error before any history existed, a manual refresh or a credential change resumes it.

## Where The Code Lives

| Piece | Location |
| --- | --- |
| Fallback module (parser, path discovery, wrapper, tests) | `src/poller/claude_desktop/usage_history.rs` |
| Module declaration | `src/poller/claude_desktop.rs`: `pub(super) mod usage_history;` |
| Hooks | `src/poller/claude.rs`: `poll_claude_code()` wraps upstream's body, now `poll_claude_code_with_credentials()`; `poll_account()` wraps `poll_account_with_credentials()` when `desktop_history_allowed(path)` |
| Custom build and stable fork release | `.github/workflows/custom-build.yml` |
| Updater follows the fork | `Cargo.toml`: `repository = "https://github.com/ridvankuntug/Claude-Code-Usage-Monitor"` |
| WinGet channel off in the fork | `src/updater.rs`: `winget_channel_supported()` / `WINGET_PACKAGE_REPOSITORY` in `current_install_channel()` |
| Upstream release workflow off in the fork | `.github/workflows/release.yml`: `if: github.repository == 'CodeZeno/Claude-Code-Usage-Monitor'` on the `build` job |
| Installer | `scripts/install-custom.ps1` |
| Fork notes and custom install | `README.md` ("Custom Fork Notes", "Installation") |

The hook is intentionally tiny:

```rust
pub(super) fn poll_claude_code() -> Result<UsageData, PollError> {
    claude_desktop::usage_history::or_desktop_history(
        poll_claude_code_with_credentials(),
        claude_desktop::usage_history::read_usage,
    )
}
```

`poll_account()` does the same through `desktop_history_allowed()`. `or_desktop_history` returns the upstream result on success or transient failure, otherwise the fallback result, otherwise the original error.

## Remotes

This local clone uses:

- `origin` for `CodeZeno/Claude-Code-Usage-Monitor` (upstream)
- `fork` for `ridvankuntug/Claude-Code-Usage-Monitor` (this fork)

## Pulling A New Upstream Version

Fetch and review upstream:

```powershell
git fetch origin
git log --oneline --decorate --max-count 20 origin/main
git diff --stat HEAD...origin/main
git diff HEAD...origin/main -- src/poller/claude.rs src/poller/claude_desktop.rs
```

Merge upstream into a working branch created from the custom branch (merge, not rebase, so the fork history stays intact):

```powershell
git switch claude-desktop-history-fallback
git switch -c story/upstream-vX.Y.Z-senkron
git merge origin/main
```

Typical conflict points and what to check:

- `src/poller/claude.rs`: keep upstream's polling bodies inside `poll_claude_code_with_credentials()` and `poll_account_with_credentials()`, and keep both wrappers. If upstream renames or splits them, hook the wrapper into whatever `PROVIDER_POLLERS` and `poller/accounts.rs` call for `ProviderId::Claude`. If upstream changes `desktop_fallback_allowed()`, keep `desktop_history_allowed()` on the same rule.
- `src/poller/claude_desktop.rs`: keep the `pub(super) mod usage_history;` line. If upstream renames `data_directories_in()`, update the call in `usage_history.rs`.
- `README.md`: take upstream's version, then reapply "Custom Fork Notes" and the custom "Installation" section.
- `.github/workflows/custom-build.yml`: keep the Rust version in sync with `rust-toolchain.toml` and `.github/workflows/release.yml`, and keep the asset name `claude-code-usage-monitor.exe` in sync with `scripts/install-custom.ps1` and `RELEASE_ASSET_NAME` in `src/updater/release.rs`.
- `Cargo.toml`: take upstream's new `version`, but keep the fork's `repository` URL. If it reverts to CodeZeno, the updater installs the official release again; `cargo test updater` fails in `fork_build_follows_fork_releases_and_never_uses_the_winget_channel`.
- `src/updater.rs`: keep the `winget_channel_supported(...)` condition in `current_install_channel()`. If upstream rewrites channel detection, reapply the same rule: the WinGet channel is used only when `repository` is CodeZeno's.
- `.github/workflows/release.yml`: keep the `if: github.repository == ...` line on the `build` job (`winget` needs `build`, so it is skipped too).

After the merge is verified, fast-forward the custom branch and push it to the fork (this triggers the custom build, which also publishes the stable `v<version>` release for the new upstream version; see [Releases And The In-App Updater](#releases-and-the-in-app-updater)):

```powershell
git switch claude-desktop-history-fallback
git merge --ff-only story/upstream-vX.Y.Z-senkron
git push fork claude-desktop-history-fallback
```

## Build And Verify

Run the same checks as CI:

```powershell
cargo fmt --check
cargo test --locked
cargo build --release --locked
```

Fallback-only tests:

```powershell
cargo test usage_history
```

They cover newest-sample selection, clamping, samples without `t`, rejected or incomplete history, path discovery and de-duplication, first-readable-file selection, that transient errors skip the fallback, and that the original error is kept when no history exists. `cargo test only_the_default_profile` covers the profile scoping.

### Machines without a full Windows SDK

`winresource` finds `rc.exe` through the `RC_PATH` environment variable, a Developer Command Prompt (`WindowsSdkVerBinPath`), or the installed Windows Kits registry entries. If none of these exist, for example a Visual Studio install with only the ScopeCppSDK, point `RC_PATH` at an `rc.exe` before building; no `build.rs` change is needed:

```powershell
$env:RC_PATH = "C:\Program Files\Microsoft Visual Studio\2022\Professional\SDK\ScopeCppSDK\vc15\SDK\bin\rc.exe"
```

Use a Visual Studio x64 developer environment (`vcvars64.bat`) for the MSVC linker. In Git Bash, make sure `/usr/bin/link` does not shadow MSVC's `link.exe`.

## Automatic Custom Builds

When `claude-desktop-history-fallback` is pushed to the fork, `.github/workflows/custom-build.yml` will (one run at a time; a newer push waits for the running one instead of cancelling it, so a release is never left half-created):

1. Run `cargo test --locked`.
2. Run `cargo build --release --locked`.
3. Move the `custom-latest` tag to the pushed commit.
4. Replace the `custom-latest` prerelease.
5. Upload `claude-code-usage-monitor.exe` as the release asset.
6. Publish the stable `v<Cargo.toml version>` release for the in-app updater if it does not exist yet (only on `claude-desktop-history-fallback`, not on manual runs from other branches).

No extra GitHub secret is required. It uses the built-in `GITHUB_TOKEN` with `contents: write`. The repository must allow it:

```text
Settings -> Actions -> General -> Workflow permissions -> Read and write permissions
```

The custom release asset is available at:

```text
https://github.com/ridvankuntug/Claude-Code-Usage-Monitor/releases/download/custom-latest/claude-code-usage-monitor.exe
```

## Releases And The In-App Updater

The updater reads `repository` from `Cargo.toml` and checks `releases/latest` of this fork. Its safeguards are unchanged from upstream: it accepts only a published, non-prerelease release whose tag is a plain SemVer version (no `-beta` part) newer than the running version, with exactly one `claude-code-usage-monitor.exe` asset whose URL is `https://github.com/ridvankuntug/Claude-Code-Usage-Monitor/releases/download/<tag>/claude-code-usage-monitor.exe`, a valid size and GitHub's `sha256:` asset digest. `custom-latest` is a prerelease with a non-SemVer tag, so the updater never sees it.

Tag strategy, implemented in the last step of `custom-build.yml`:

- The tag is `v` + the `Cargo.toml` version, e.g. `v2.22.4`, the same number as the upstream release it is based on. The release is marked `--latest`, not prerelease, and uses the same asset name.
- If the release already exists, the step leaves it unchanged (no re-tagging, no asset replacement). A draft with that tag, or a tag without a release, fails the step for manual cleanup; tags are never moved automatically.
- By policy, a version with pre-release or build metadata (`2.23.0-beta1`, `2.23.0+fork`) is skipped with a warning. The updater never installs pre-releases, and build metadata is ignored in SemVer ordering, so adding only `+meta` to an existing X.Y.Z never makes a release newer; plain X.Y.Z tags also keep the release URL simple.
- `gh release create --target <commit>` creates the tag through the API with `GITHUB_TOKEN`, so it does not trigger other workflows. `release.yml` is skipped in the fork anyway.

Consequences:

- **Fork-only changes without a version bump do not reach the updater.** They are published only to `custom-latest`. To ship them through the updater, raise `version` in `Cargo.toml` (this also updates the package entry in `Cargo.lock`; commit both) and push the custom branch. If you take the next patch number (e.g. `2.22.5`), the later upstream `2.22.5` will find that release existing and will not be published until you bump past it, and the fork's version number then no longer matches upstream. The simplest option is to wait for the next upstream version.
- **After an upstream merge** (new upstream `version`): push `claude-desktop-history-fallback` to the fork; the run publishes `v<new version>` as latest and existing fork installs offer it.
- Do not push upstream tags to the fork (`git push fork --tags` or `--follow-tags` from this clone pushes CodeZeno's `v*` tags). A `v<version>` tag that exists without a release blocks that version's stable release.
- Fork builds made before the updater pointed at the fork still check the CodeZeno releases. Replace them once manually with `custom-latest`.

WinGet: the package `CodeZeno.ClaudeCodeUsageMonitor` is the official release. Fork builds always use the portable GitHub channel, even when the exe is inside a WinGet package directory, so the app never offers a WinGet upgrade to the official release. Do not install or upgrade this fork through WinGet.

## Downloading And Installing

Plain exe:

```powershell
irm https://github.com/ridvankuntug/Claude-Code-Usage-Monitor/releases/download/custom-latest/claude-code-usage-monitor.exe -OutFile .\claude-code-usage-monitor.exe
.\claude-code-usage-monitor.exe --diagnose
```

Optional install helper from a local clone:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-custom.ps1 -Repo ridvankuntug/Claude-Code-Usage-Monitor
```

Directly from GitHub:

```powershell
$installer = Join-Path $env:TEMP "install-claude-code-usage-monitor-custom.ps1"; irm https://raw.githubusercontent.com/ridvankuntug/Claude-Code-Usage-Monitor/claude-desktop-history-fallback/scripts/install-custom.ps1 -OutFile $installer; powershell -ExecutionPolicy Bypass -File $installer -Repo ridvankuntug/Claude-Code-Usage-Monitor
```

## Runtime Smoke Test

Run the custom executable with diagnostics and enable recording in the dashboard's **Diagnostics** page if needed:

```powershell
.\claude-code-usage-monitor.exe --diagnose
```

When upstream polling fails and the fallback is used, the log shows lines like:

```text
Claude usage poll failed: NoCredentials; trying Claude Desktop usage history
using Claude Desktop usage history at ...\Claude-Data\claude_...\plan-usage-history.json
```

When no history is usable:

```text
Claude Desktop usage history not found
```

Do not reinstall with WinGet: it installs the official CodeZeno release, which replaces the fallback behavior. The in-app updater follows `repository` in `Cargo.toml` (this fork), so accepting its updates keeps the fallback.
