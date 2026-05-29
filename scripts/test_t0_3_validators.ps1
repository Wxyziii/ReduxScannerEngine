# T0.3 scope-validator regression CLI test script
# Requires: cargo in PATH and the patch plan file at the given absolute path

$patchPlan = "C:\Users\Marcel\Downloads\valid_dark_grey_cloudy_sky_qwen.patch_plan.json"
if (-not (Test-Path $patchPlan)) {
    Write-Host "Patch plan not found at $patchPlan" -ForegroundColor Red
    exit 2
}

$cases = @{
    "valid_first_patch" = @{ changed = "visualsettings.dat,cloudkeyframes.xml,timecycle_mods_1.xml"; expectOk = $true }
    "phase_1_2_changed"  = @{ changed = "visualsettings.dat,w_foggy.xml"; expectOk = $false }
    "deferred_global"    = @{ changed = "weather.xml"; expectOk = $false }
    "blocked_file"       = @{ changed = "timecycle_mods_3.xml"; expectOk = $false }
    "binary_file"        = @{ changed = "cloudkeyframes.xml,some_texture.ytd"; expectOk = $false }
    "rpf_archive"        = @{ changed = "update.rpf"; expectOk = $false }
    "unrelated_component"= @{ changed = "tracer_effect.xml"; expectOk = $false }
}

$failures = @()
foreach ($k in $cases.Keys) {
    $entry = $cases[$k]
    $changed = $entry.changed
    $expectOk = $entry.expectOk

    Write-Host "\n=== Running: $k ==="
    $cmd = "cargo run --manifest-path rpf_backend_rs\Cargo.toml -- validate-scope --patch-plan \"$patchPlan\" --changed-files $changed"
    Write-Host $cmd

    $out = & cmd /c $cmd 2>&1
    $raw = $out -join "`n"

    try {
        $json = $raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        Write-Host "Failed to parse JSON output for $k:" -ForegroundColor Red
        Write-Host $raw
        $failures += $k
        continue
    }

    $ok = $json.ok
    if ($ok -ne $expectOk) {
        Write-Host "Unexpected result for $k: expected ok=$expectOk but got ok=$ok" -ForegroundColor Red
        Write-Host "$raw"
        $failures += $k
    } else {
        Write-Host "Result matched expected for $k: ok=$ok" -ForegroundColor Green
    }
}

if ($failures.Count -eq 0) {
    Write-Host "All CLI regression cases matched expectations." -ForegroundColor Green
    exit 0
} else {
    Write-Host "Failures: $($failures -join ', ')" -ForegroundColor Red
    exit 1
}
