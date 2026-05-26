$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root "dist"
$Tools = Join-Path $Dist "tools"

New-Item -ItemType Directory -Force -Path $Dist, $Tools | Out-Null

Write-Host "Building Rust backend..."
Push-Location (Join-Path $Root "rpf_backend_rs")
cargo build --release
Pop-Location

Write-Host "Building C++ launcher..."
$Cpp = Join-Path $Root "src\cpp\redux_rpf_scanner.cpp"
$Out = Join-Path $Dist "redux_rpf_scanner.exe"

$gpp = Get-Command g++ -ErrorAction SilentlyContinue
if ($gpp) {
    & $gpp.Source -std=c++17 -O2 $Cpp -o $Out
} else {
    Write-Host "g++ not found. Trying CMake..."
    cmake -S $Root -B (Join-Path $Root "build")
    cmake --build (Join-Path $Root "build") --config Release
    $built = Get-ChildItem (Join-Path $Root "build") -Recurse -Filter "redux_rpf_scanner.exe" | Select-Object -First 1
    if (-not $built) { throw "Could not find built redux_rpf_scanner.exe" }
    Copy-Item $built.FullName $Out -Force
}

$BackendSrc = Join-Path $Root "rpf_backend_rs\target\release\rpf_backend_rs.exe"
$BackendDst = Join-Path $Tools "rpf_backend_rs.exe"
Copy-Item $BackendSrc $BackendDst -Force

Write-Host "Done."
Write-Host "Dist:"
Write-Host $Dist
