# Claude Desktop History Fallback Maintenance

This note is for carrying the local Claude Desktop usage history fallback forward after upstream updates.

## Goal

Keep this behavior available even after pulling new versions from `CodeZeno/Claude-Code-Usage-Monitor`:

- Try the existing Claude Code OAuth usage polling first.
- If OAuth polling fails, read Claude Desktop's local `plan-usage-history.json`.
- Read only these history files:
  - `%APPDATA%\Claude\plan-usage-history.json`
  - `%LOCALAPPDATA%\Claude\plan-usage-history.json`
  - `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude\plan-usage-history.json`
- Do not read Claude Desktop tokens, cookies, Local Storage, or `Network\Cookies`.
- Pick the sample with the highest `t`.
- Map `u.fh` to `session.percentage`.
- Map `u.sd` to `weekly.percentage`.
- Clamp both percentages to `0..100`.
- Leave both `resets_at` values as `None`.

## Before Pulling Upstream

Save the current local fallback change as a patch:

```powershell
git diff -- src\poller.rs README.md docs\claude-desktop-history-fallback-maintenance.md > claude-desktop-history-fallback.patch
```

Optionally keep a copy outside the repository too:

```powershell
Copy-Item -Force .\claude-desktop-history-fallback.patch ..\claude-desktop-history-fallback.patch
```

Check what is currently modified:

```powershell
git status --short
```

## Pulling A New Upstream Version

If this repository still points at upstream, fetch the latest changes:

```powershell
git fetch origin
```

Review what changed upstream before applying anything:

```powershell
git log --oneline --decorate --max-count 20 origin/main
git diff --stat HEAD..origin/main
git diff HEAD..origin/main -- src\poller.rs
```

Then update the working tree using your preferred local workflow. If you do not want to commit local changes, keep the patch file and reclone into a fresh folder.

## Reapplying The Fallback

After updating or recloning, apply the saved patch:

```powershell
git apply --reject --whitespace=nowarn .\claude-desktop-history-fallback.patch
```

If the patch applies cleanly, inspect the result:

```powershell
git diff -- src\poller.rs
```

If `.rej` files appear, reapply the change manually in `src\poller.rs`.

## Manual Reapply Checklist

In `src/poller.rs`, keep these pieces:

1. Claude Desktop history structs:
   - `ClaudeDesktopPlanUsageHistory`
   - `ClaudeDesktopPlanUsageSample`
   - `ClaudeDesktopPlanUsage`

2. `poll_claude_code()` wrapper:

```rust
fn poll_claude_code() -> Result<UsageData, PollError> {
    match poll_claude_code_oauth() {
        Ok(data) => Ok(data),
        Err(error) => {
            diagnose::log(format!(
                "Claude Code OAuth usage poll failed: {error:?}; trying Claude Desktop history"
            ));
            read_claude_desktop_usage().ok_or(error)
        }
    }
}
```

3. Keep the original OAuth logic in:

```rust
fn poll_claude_code_oauth() -> Result<UsageData, PollError> {
    // original Claude Code OAuth polling logic
}
```

4. History file discovery:
   - `%APPDATA%\Claude\plan-usage-history.json`
   - `%LOCALAPPDATA%\Claude\plan-usage-history.json`
   - `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude\plan-usage-history.json`

5. Parser behavior:

```rust
fn claude_desktop_usage_from_history(content: &str) -> Option<UsageData> {
    let history: ClaudeDesktopPlanUsageHistory = serde_json::from_str(content).ok()?;
    let sample = history.samples.into_iter().max_by_key(|sample| sample.t)?;

    Some(UsageData {
        session: UsageSection {
            percentage: sample.u.fh?.clamp(0.0, 100.0),
            resets_at: None,
        },
        weekly: UsageSection {
            percentage: sample.u.sd?.clamp(0.0, 100.0),
            resets_at: None,
        },
    })
}
```

6. Tests:
   - newest sample is selected by highest `t`
   - percentages are clamped to `0..100`

## Build And Verify

Use the Visual Studio x64 build environment:

```powershell
cmd.exe /d /s /c '"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && set CARGO_HTTP_CHECK_REVOKE=false&& "%USERPROFILE%\.cargo\bin\cargo.exe" test'
cmd.exe /d /s /c '"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && set CARGO_HTTP_CHECK_REVOKE=false&& "%USERPROFILE%\.cargo\bin\cargo.exe" build --release'
```

Copy the portable executable:

```powershell
New-Item -ItemType Directory -Force -Path .\outputs | Out-Null
Copy-Item -Force .\target\release\claude-code-usage-monitor.exe .\outputs\claude-code-usage-monitor.exe
```

Confirm line endings and whitespace:

```powershell
git diff --check
$bytes = [System.IO.File]::ReadAllBytes('src\poller.rs'); $lf=0; $crlf=0; for($i=0;$i -lt $bytes.Length;$i++){ if($bytes[$i] -eq 10){ $lf++; if($i -gt 0 -and $bytes[$i-1] -eq 13){$crlf++} } }; "LF=$lf CRLF=$crlf"
```

## Runtime Smoke Test

Run the custom executable directly instead of installing with WinGet:

```powershell
.\outputs\claude-code-usage-monitor.exe --diagnose
```

Look for logs like:

```text
Claude Code OAuth usage poll failed: NoCredentials; trying Claude Desktop history
using Claude Desktop usage history at ...\Packages\Claude_...\LocalCache\Roaming\Claude\plan-usage-history.json
```

Do not use WinGet to reinstall for this custom build. WinGet installs the official release and will replace this local fallback behavior unless upstream has accepted the same change.

