param(
    [Parameter(Mandatory = $true)]
    [string]$Repo,

    [string]$Tag = "custom-latest",

    [string]$InstallDir = "$env:LOCALAPPDATA\ClaudeCodeUsageMonitorCustom"
)

$ErrorActionPreference = "Stop"

if ($Repo -notmatch "^[^/]+/[^/]+$") {
    throw "Repo must be in owner/name format, for example ridvankuntug/Claude-Code-Usage-Monitor."
}

$exeName = "claude-code-usage-monitor.exe"
$downloadUrl = "https://github.com/$Repo/releases/download/$Tag/$exeName"
$exePath = Join-Path $InstallDir $exeName

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

Write-Host "Downloading $downloadUrl"
Invoke-WebRequest -Uri $downloadUrl -OutFile $exePath

$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$shortcutPath = Join-Path $startMenuDir "Claude Code Usage Monitor Custom.lnk"

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $exePath
$shortcut.WorkingDirectory = $InstallDir
$shortcut.Description = "Claude Code Usage Monitor custom build"
$shortcut.Save()

Write-Host "Installed to $exePath"
Write-Host "Start Menu shortcut: $shortcutPath"
Write-Host ""
Write-Host "Run it with:"
Write-Host "`"$exePath`" --diagnose"

