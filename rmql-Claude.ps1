# Starts Claude Code in this folder (the rmql repository), wherever it was downloaded.
# It continues the latest conversation of this PC; where there is none (a new PC), it starts a new
# one, and CLAUDE.md ("状態と引継ぎ") carries the work over.
#
# Usage: right-click > Run with PowerShell, or `.\rmql-Claude.ps1` (or rmql-Claude.bat).
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
$host.UI.RawUI.WindowTitle = 'rmql - Claude Code'

$claude = (Get-Command claude -ErrorAction SilentlyContinue).Source
if (-not $claude) {
    $local = Join-Path $env:USERPROFILE '.local\bin\claude.exe'
    if (Test-Path -LiteralPath $local) { $claude = $local }
}
if (-not $claude) {
    Write-Host 'Claude Code was not found. Install it first: https://claude.com/claude-code' -ForegroundColor Red
    Read-Host 'Press Enter to close'
    exit 1
}

# --continue stops with an error when this PC has no conversation for this folder.
& $claude --continue
if ($LASTEXITCODE -ne 0) {
    Write-Host 'No conversation to continue here; starting a new one (read CLAUDE.md first).' -ForegroundColor Yellow
    & $claude
}
