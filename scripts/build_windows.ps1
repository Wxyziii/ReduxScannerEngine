$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root "dist"
$Tools = Join-Path $Dist "tools"
$RulesDist = Join-Path $Dist "rules"
$BuildDir = Join-Path $Root "build"

New-Item -ItemType Directory -Force -Path $Dist, $Tools, $RulesDist | Out-Null

Write-Host "[build] Building Rust backend..."
Push-Location (Join-Path $Root "rpf_backend_rs")
cargo build --release
Pop-Location

Write-Host "[build] Building C++ launcher..."
$Cpp = Join-Path $Root "src\cpp\redux_rpf_scanner.cpp"
$Out = Join-Path $Dist "redux_rpf_scanner.exe"

$gpp = Get-Command g++ -ErrorAction SilentlyContinue
if ($gpp) {
    & $gpp.Source -std=c++17 -O2 $Cpp -o $Out
} else {
    Write-Host "[build] g++ not found. Trying CMake..."
    cmake -S $Root -B $BuildDir
    cmake --build $BuildDir --config Release
    $built = Get-ChildItem $BuildDir -Recurse -Filter "redux_rpf_scanner.exe" | Select-Object -First 1
    if (-not $built) { throw "Could not find built redux_rpf_scanner.exe" }
    Copy-Item $built.FullName $Out -Force
}

Write-Host "[build] Copying Rust backend..."
$BackendSrc = Join-Path $Root "rpf_backend_rs\target\release\rpf_backend_rs.exe"
$BackendDst = Join-Path $Tools "rpf_backend_rs.exe"
Copy-Item $BackendSrc $BackendDst -Force

Write-Host "[build] Copying example rules..."
Get-ChildItem (Join-Path $Root "rules") -Filter "*.example.json" | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $RulesDist $_.Name) -Force
}

$RuntimeReadme = Join-Path $Root "README_RUNTIME.md"
if (Test-Path $RuntimeReadme) {
    Write-Host "[build] Copying README_RUNTIME.md..."
    Copy-Item $RuntimeReadme (Join-Path $Dist "README_RUNTIME.md") -Force
} else {
    Write-Host "[build] README_RUNTIME.md not found. Skipping copy."
}

Write-Host "[build] Done."
Write-Host "[build] Dist tree:"
Get-ChildItem -Recurse $Dist
