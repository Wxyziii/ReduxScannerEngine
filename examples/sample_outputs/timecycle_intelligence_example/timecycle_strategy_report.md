# Timecycle Strategy Report

## Overview
- sanitized example only; all values below are fictional.
- scope: sky/timecycle planning from fake scanner evidence.
- language remains cautious because scanner evidence is indirect.

## Strongest timecycle candidates
- `visualsettings.dat` — high confidence; sampled named keys exist.
- `cloudkeyframes.xml` — medium confidence; color-like changes are prominent.
- `w_foggy.xml` — medium confidence; weather-specific scope is narrower.

## Safest first-patch candidates
- `visualsettings.dat` one named key at a time.
- `cloudkeyframes.xml` color-like values only.
- `w_foggy.xml` or `w_clouds.xml` color-only weather tint changes.

## Risky/deferred files
- `timecycle_mods_3.xml` — schema unknown.
- `weather.xml` — likely global.
- `timecycle_mods_4.xml` — linked to kill_effect scope.

## Evidence from scanner data
- `visualsettings.dat`: changedKeyCount=4, numericChanges=12.
- `cloudkeyframes.xml`: numericChanges=42, colorLikeChanges=88.
- `w_foggy.xml`: numericChanges=18, colorLikeChanges=25.

## Recommended first patch scope
Start with one file, one narrow operation, and validate before any expansion.

## Validation requirements
- parse must remain valid.
- no unexpected node deletion.
- in-game screenshots required before claiming success.

## What AI may infer vs must not infer
### AI may infer
- priority order.
- candidate-first scope.
- validation needs.

### AI must not infer
- exact visual outcomes.
- schema meanings not present in names.
- direct binary edits.

## Deterministic tools needed
- `dat_config_patcher`
- `xml_cloudkeyframe_editor`
- `xml_weather_editor`
