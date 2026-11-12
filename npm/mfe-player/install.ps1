# mfe-player PowerShell installer (Windows 10/11).
#
#   irm https://raw.githubusercontent.com/omkargs/mfe-player/master/npm/mfe-player/install.ps1 | iex
#
# Installs the latest release into %LOCALAPPDATA%\mfe-player\bin, adds it to the
# user PATH, writes PowerShell completions and checks for mpv/yt-dlp/python.
# No admin rights, no registry writes outside HKCU.
[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$Prefix = (Join-Path $env:LOCALAPPDATA "mfe-player"),
    [switch]$NoPath,
    [switch]$NoDeps
)

$ErrorActionPreference = "Stop"
$Repo = "omkargs/mfe-player"
$BinDir = Join-Path $Prefix "bin"
$Exe = Join-Path $BinDir "mfe-player.exe"

function Say($message) { Write-Host "  > $message" -ForegroundColor Cyan }
function Ok($message) { Write-Host "  + $message" -ForegroundColor Green }
function Warn($message) { Write-Host "  ! $message" -ForegroundColor Yellow }
function Die($message) { Write-Host "  x $message" -ForegroundColor Red; exit 1 }

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "ARM64" { "aarch64" }
    "AMD64" { "x86_64" }
    default { Die "unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
}
$triple = "$arch-pc-windows-msvc"

$base = if ($Version -eq "latest") {
    "https://github.com/$Repo/releases/latest/download"
} else {
    "https://github.com/$Repo/releases/download/v$Version"
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
$zip = Join-Path $env:TEMP "mfe-player-$triple.zip"
$latest = Join-Path $env:TEMP "mfe-player-latest.json"
$url = "$base/mfe-player-$triple.zip"
$expected = $null

Say "resolving $Version release for $triple"
try {
    Invoke-WebRequest -UseBasicParsing -Uri "$base/latest.json" -OutFile $latest
    $manifest = Get-Content -Raw $latest | ConvertFrom-Json
    $entry = $manifest.targets.$triple
    if ($entry) {
        if ($entry.url) { $url = $entry.url }
        $expected = $entry.sha256
    }
} catch {
    Warn "latest.json unavailable; using the direct release URL"
}

Say "downloading $url"
Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $zip

if ($expected) {
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash.ToLowerInvariant()
    if ($actual -ne $expected.ToLowerInvariant()) {
        Die "checksum mismatch (expected $expected, got $actual)"
    }
    Ok "checksum verified"
} else {
    Warn "no checksum published for this release"
}

Say "installing to $BinDir"
Expand-Archive -Force -LiteralPath $zip -DestinationPath $BinDir
Remove-Item -Force $zip, $latest -ErrorAction SilentlyContinue

if (-not (Test-Path $Exe)) { Die "mfe-player.exe was not found after extraction" }
Ok "installed $(& $Exe --version)"

if (-not $NoPath) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @($userPath -split ";" | Where-Object { $_ })
    if ($parts -notcontains $BinDir) {
        $newPath = (@($parts) + $BinDir) -join ";"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        $env:Path = "$env:Path;$BinDir"
        Ok "added $BinDir to the user PATH (open a new terminal to pick it up)"
    } else {
        Ok "PATH already contains $BinDir"
    }
}

if (-not $NoDeps) {
    $missing = @()
    foreach ($tool in @("mpv", "yt-dlp", "python", "ffmpeg")) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { $missing += $tool }
    }
    if ($missing.Count -gt 0) {
        Warn "missing: $($missing -join ', ')"
        Write-Host "  install them with winget:" -ForegroundColor Yellow
        Write-Host "    winget install mpv yt-dlp Python.Python.3.12 Gyan.FFmpeg" -ForegroundColor DarkGray
        if (Get-Command winget -ErrorAction SilentlyContinue) {
            $answer = Read-Host "  install them now with winget? [y/N]"
            if ($answer -match "^(y|yes)$") {
                & winget install --accept-source-agreements --accept-package-agreements mpv yt-dlp Python.Python.3.12 Gyan.FFmpeg
            }
        }
    } else {
        Ok "mpv, yt-dlp, python and ffmpeg are present"
    }
}

# PowerShell completions: register once, then dot-source from the profile.
try {
    $completionDir = Join-Path $env:USERPROFILE "Documents\PowerShell"
    New-Item -ItemType Directory -Force -Path $completionDir | Out-Null
    $completionFile = Join-Path $completionDir "mfe-completions.ps1"
    & $Exe completions powershell | Set-Content -Encoding UTF8 -Path $completionFile
    $profilePath = $PROFILE.CurrentUserAllHosts
    $line = ". `"$completionFile`""
    if (-not (Test-Path $profilePath)) {
        New-Item -ItemType File -Force -Path $profilePath | Out-Null
    }
    if (-not (Select-String -Quiet -SimpleMatch $line -Path $profilePath)) {
        Add-Content -Path $profilePath -Value "`n# mfe-player completions`n$line"
    }
    Ok "PowerShell completions registered"
} catch {
    Warn "could not register completions: $($_.Exception.Message)"
}

Say "running mfe doctor"
& $Exe doctor
exit 0
