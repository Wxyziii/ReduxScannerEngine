# Sky/Timecycle AI Context — Redux Scanner

## Scan Summary
Sanitized example only. The scanner compared fictional clean-vs-modded data and summarized likely sky/timecycle candidates. All interpretations remain hypotheses and require validation.

## Key Findings
- `visualsettings.dat` contains readable named keys.
- `cloudkeyframes.xml` shows mixed numeric and color-like deltas.
- `w_foggy.xml` and `w_clouds.xml` look like narrower weather candidates.
- `timecycle_mods_3.xml` remains deferred because schema is unknown.
- `weather.xml` stays deferred because it may be global.

## Ranked Candidate Files
1. `visualsettings.dat` — named key evidence; first patch candidate.
2. `cloudkeyframes.xml` — strong sky naming plus color-like deltas.
3. `w_foggy.xml` — narrower weather scope.
4. `w_clouds.xml` — narrower weather scope.

## Safest First-Patch Scope
- one named-key change in `visualsettings.dat`
- color-only pass in `cloudkeyframes.xml`
- weather tint-only pass in `w_foggy.xml` or `w_clouds.xml`

## Risky / Deferred Files
- `timecycle_mods_3.xml`
- `weather.xml`
- binary families such as `.ytd`, `.ypt`, `.ysc`, `.gfx`, `.fxc`
- unrelated components such as tracer, hit_effect, minimap_hud, kill_effect

## Validation Rules
- keep every patch narrow and reversible
- require parse success after each edit
- do not delete nodes unexpectedly
- require in-game screenshot validation

## Tool Requirements
- `dat_config_patcher`
- `xml_cloudkeyframe_editor`
- `xml_weather_editor`
- XML parser / diff validator

## AI Must Not
- invent exact meanings not present in names
- propose binary edits
- claim exact visual outcomes without validation
- mix unrelated components into a first patch

## AI May
- rank candidate files
- explain evidence from counts and names
- recommend a cautious first-patch order
- propose validation checklists
