$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root "dist"

& (Join-Path $PSScriptRoot "build_windows.ps1")

Write-Host "Package complete. Dist: $Dist"
