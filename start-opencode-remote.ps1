#Requires -Version 5.1
<#
.SYNOPSIS
    Starts opencode's server mode bound to all network interfaces, so other
    devices on the same LAN/Wi-Fi can connect to it (e.g. from a phone or
    another computer's browser, or from a remote opencode/TUI client
    pointed at this machine's IP).

.DESCRIPTION
    Requires opencode to be installed (https://opencode.ai). This script
    does NOT install it - install it first, then re-run this script.

.PARAMETER Port
    Port to listen on (default: 4096).

.PARAMETER ProjectPath
    Folder opencode should treat as its project root - this is *the* fix
    for "I can only reach C:\": opencode uses its own current working
    directory as the project root, so if this script (or a shortcut to it)
    gets launched with some other folder as the working directory (very
    common for shortcuts / Start Menu / Task Scheduler - they often default
    to C:\Windows\System32 or C:\), opencode ends up rooted at that folder
    instead of your actual project. Defaults to E:\GitHub (your projects
    folder) - pass a different path to root it somewhere else instead.

.PARAMETER ExtraArgs
    Extra arguments passed through to `opencode serve` as-is.

.EXAMPLE
    .\start-opencode-remote.ps1
.EXAMPLE
    .\start-opencode-remote.ps1 -Port 8080 -ProjectPath "C:\Users\me\my-project"

.NOTES
    Security: this exposes opencode (and therefore whatever shell/file
    access it has on this machine) to every device on your local network.
    Only run this on a network you trust, and stop it (Ctrl+C) when done.
    Windows Firewall may prompt to allow the connection the first time -
    accept it only for Private networks, not Public ones.
#>
param(
    [int]$Port = 4096,
    [string]$ProjectPath = "E:\GitHub",
    [string[]]$ExtraArgs = @()
)

$ErrorActionPreference = "Stop"

$opencode = Get-Command opencode -ErrorAction SilentlyContinue
if (-not $opencode) {
    Write-Host "error: 'opencode' was not found on PATH." -ForegroundColor Red
    Write-Host "Install it first - see https://opencode.ai - then re-run this script." -ForegroundColor Red
    exit 1
}

if (-not (Test-Path -LiteralPath $ProjectPath -PathType Container)) {
    Write-Host "error: project folder not found: $ProjectPath" -ForegroundColor Red
    exit 1
}
$ProjectPath = (Resolve-Path -LiteralPath $ProjectPath).Path
Set-Location -LiteralPath $ProjectPath

# Best-effort LAN IP detection, for printing a connectable URL. Picks the
# first non-loopback, non-link-local IPv4 address from an "up" adapter -
# usually your Wi-Fi or Ethernet adapter. If detection fails, the script
# still starts the server; find your IP manually with `ipconfig`.
function Get-LanIPAddress {
    try {
        $candidates = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction Stop |
            Where-Object {
                $_.IPAddress -notlike "127.*" -and
                $_.IPAddress -notlike "169.254.*" -and
                $_.PrefixOrigin -ne "WellKnown"
            }
        $preferred = $candidates | Where-Object {
            (Get-NetAdapter -InterfaceIndex $_.InterfaceIndex -ErrorAction SilentlyContinue).Status -eq "Up"
        } | Select-Object -First 1
        if ($preferred) { return $preferred.IPAddress }
        if ($candidates) { return ($candidates | Select-Object -First 1).IPAddress }
    } catch {
        # Get-NetIPAddress isn't available on older systems - fall through.
    }
    return $null
}

$lanIp = Get-LanIPAddress

Write-Host "Project root (opencode's working directory): $ProjectPath" -ForegroundColor Cyan
Write-Host "Starting opencode server on 0.0.0.0:$Port ..."
if ($lanIp) {
    Write-Host "Reachable from other devices on this network at: http://${lanIp}:${Port}" -ForegroundColor Green
} else {
    Write-Host "Could not auto-detect this machine's LAN IP - run 'ipconfig' to find it," -ForegroundColor Yellow
    Write-Host "then connect to http://<that-ip>:$Port from another device." -ForegroundColor Yellow
}
Write-Host "Press Ctrl+C to stop."
Write-Host ""

& opencode serve --hostname 0.0.0.0 --port $Port @ExtraArgs
