#Requires -Version 5.1
<#
.SYNOPSIS
    Installs the Anchor CLI for the current Windows user. No administrator
    rights are required; the binary is installed under the user's local
    app-data directory by default.

.PARAMETER Version
    Release tag to install (e.g. "v0.1.7"). Defaults to the latest release,
    or to $env:ANCHOR_VERSION when set.

.PARAMETER InstallDir
    Directory to install anchor.exe into. Defaults to
    "$env:LOCALAPPDATA\Programs\Anchor\bin", or to
    $env:ANCHOR_INSTALL_DIR when set.

.PARAMETER SkipPathUpdate
    Do not add InstallDir to the current user's PATH environment variable.
#>
[CmdletBinding()]
param(
    [string]$Version = $env:ANCHOR_VERSION,
    [string]$InstallDir = $(if ($env:ANCHOR_INSTALL_DIR) { $env:ANCHOR_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\Anchor\bin' }),
    [switch]$SkipPathUpdate
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Repo = 'HyperMarble/Anchor'
$Archive = 'anchor-windows-x64.zip'
$RequestHeaders = @{ 'User-Agent' = 'anchor-installer' }

function Resolve-AnchorVersion {
    param([string]$Requested)

    if ($Requested) {
        return $Requested
    }

    Write-Host "Resolving latest release..."
    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases" -Headers $RequestHeaders -UseBasicParsing
    if (-not $releases -or $releases.Count -eq 0) {
        throw "No releases found for $Repo"
    }
    $latest = $releases | Select-Object -First 1
    return $latest.tag_name
}

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("anchor-install-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

try {
    $resolvedVersion = Resolve-AnchorVersion -Requested $Version
    Write-Host "Version: $resolvedVersion"

    $zipUrl = "https://github.com/$Repo/releases/download/$resolvedVersion/$Archive"
    $shaUrl = "$zipUrl.sha256"

    $zipPath = Join-Path $tempDir $Archive
    $shaPath = Join-Path $tempDir "$Archive.sha256"

    Write-Host "Downloading $Archive..."
    Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath -Headers $RequestHeaders -UseBasicParsing
    Invoke-WebRequest -Uri $shaUrl -OutFile $shaPath -Headers $RequestHeaders -UseBasicParsing

    Write-Host "Verifying checksum..."
    $expectedLine = (Get-Content -Path $shaPath -Raw).Trim()
    if (-not $expectedLine) {
        throw "Empty checksum file: $shaUrl"
    }
    $expectedHash = ($expectedLine -split '\s+')[0].ToLowerInvariant()
    if ($expectedHash -notmatch '^[0-9a-f]{64}$') {
        throw "Malformed checksum file: $shaUrl"
    }
    $actualHash = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($expectedHash -ne $actualHash) {
        throw "Checksum mismatch for $Archive (expected $expectedHash, got $actualHash)"
    }

    Write-Host "Extracting..."
    $extractDir = Join-Path $tempDir 'extracted'
    Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

    $exeSource = Join-Path $extractDir 'anchor.exe'
    $lockdSource = Join-Path $extractDir 'anchor-lockd.exe'
    $licenseSource = Join-Path $extractDir 'LICENSE'
    foreach ($requiredFile in @($exeSource, $lockdSource, $licenseSource)) {
        if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
            throw "Downloaded archive is missing '$([System.IO.Path]::GetFileName($requiredFile))'"
        }
    }

    Write-Host "Installing Anchor -> $InstallDir"
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $exeDest = Join-Path $InstallDir 'anchor.exe'
    $lockdDest = Join-Path $InstallDir 'anchor-lockd.exe'
    $licenseDest = Join-Path $InstallDir 'LICENSE'
    Copy-Item -Path $exeSource -Destination $exeDest -Force
    Copy-Item -Path $lockdSource -Destination $lockdDest -Force
    Copy-Item -Path $licenseSource -Destination $licenseDest -Force

    if (-not $SkipPathUpdate) {
        $userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
        $segments = @()
        if ($userPath) {
            $segments = $userPath -split ';' | Where-Object { $_ -ne '' }
        }
        $alreadyPresent = $segments | Where-Object { $_.TrimEnd('\') -ieq $InstallDir.TrimEnd('\') }
        if (-not $alreadyPresent) {
            $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
            [Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
            Write-Host "Added $InstallDir to your user PATH (open a new terminal to pick it up)."
        } else {
            Write-Host "$InstallDir already on PATH."
        }
        if (($env:Path -split ';') -notcontains $InstallDir) {
            $env:Path = "$env:Path;$InstallDir"
        }
    } else {
        Write-Host "Skipped PATH update. Add $InstallDir to PATH manually if desired."
    }

    Write-Host "Smoke testing anchor.exe..."
    & $exeDest --help | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Smoke test failed: 'anchor --help' exited with code $LASTEXITCODE"
    }
    & $lockdDest -h 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "Smoke test failed: 'anchor-lockd -h' exited with code $LASTEXITCODE"
    }

    Write-Host ""
    Write-Host " █████╗ ███╗   ██╗ ██████╗██╗  ██╗ ██████╗ ██████╗"
    Write-Host "██╔══██╗████╗  ██║██╔════╝██║  ██║██╔═══██╗██╔══██╗"
    Write-Host "███████║██╔██╗ ██║██║     ███████║██║   ██║██████╔╝"
    Write-Host "██╔══██║██║╚██╗██║██║     ██╔══██║██║   ██║██╔══██╗"
    Write-Host "██║  ██║██║ ╚████║╚██████╗██║  ██║╚██████╔╝██║  ██║"
    Write-Host "╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝"
    Write-Host "       Agent-Code Interface"
    Write-Host ""
    Write-Host "Get started:"
    Write-Host "  cd your-project"
    Write-Host "  anchor check -- <test-command>"
    Write-Host "  anchor status"
    Write-Host "  anchor receipt"
    Write-Host "  anchor gate --min-score 85"
    Write-Host ""
    Write-Host "Uninstall: irm https://raw.githubusercontent.com/$Repo/main/docs/uninstall.ps1 | iex"
    Write-Host ""
}
finally {
    if (Test-Path $tempDir) {
        Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
