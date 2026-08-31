#Requires -Version 5.1
<#
.SYNOPSIS
    Uninstalls the Anchor CLI that was installed by docs/install.ps1.

.PARAMETER InstallDir
    Directory anchor.exe was installed into. Defaults to
    "$env:LOCALAPPDATA\Programs\Anchor\bin", or to
    $env:ANCHOR_INSTALL_DIR when set.
#>
[CmdletBinding()]
param(
    [string]$InstallDir = $(if ($env:ANCHOR_INSTALL_DIR) { $env:ANCHOR_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\Anchor\bin' })
)

$ErrorActionPreference = 'Stop'

$installedFiles = @('anchor.exe', 'anchor-lockd.exe', 'LICENSE')
$removedAny = $false
foreach ($fileName in $installedFiles) {
    $filePath = Join-Path $InstallDir $fileName
    if (Test-Path -LiteralPath $filePath -PathType Leaf) {
        Remove-Item -LiteralPath $filePath -Force
        Write-Host "Removed $filePath"
        $removedAny = $true
    }
}
if (-not $removedAny) {
    Write-Host "Anchor not found in $InstallDir"
}

if ((Test-Path $InstallDir) -and -not (Get-ChildItem -Path $InstallDir -Force -ErrorAction SilentlyContinue)) {
    Remove-Item -Path $InstallDir -Force -ErrorAction SilentlyContinue
}

$userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
if ($userPath) {
    $segments = $userPath -split ';' | Where-Object { $_ -ne '' -and $_.TrimEnd('\') -ine $InstallDir.TrimEnd('\') }
    $newPath = $segments -join ';'
    if ($newPath -ne $userPath) {
        [Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
        Write-Host "Removed $InstallDir from your user PATH."
    }
}

Write-Host ""
Write-Host "Anchor uninstalled."
Write-Host ""
