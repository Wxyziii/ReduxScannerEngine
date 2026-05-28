# Patch Plan Schema v1

This document defines the strict machine-readable format for Redux Maker patch plans.

## Schema Definition

- **Version:** `patch-plan/v1`
- **Format:** JSON

### Top-Level Fields

| Field | Type | Description |
|-------|------|-------------|
| `schemaVersion` | `string` | Must be `patch-plan/v1`. |
| `planType` | `string` | e.g., `timecycle_patch`. |
| `goal` | `object` | User prompt and normalized goal. |
| `sourceContext` | `object` | Scanner reports and evidence summary. |
| `safetyPolicy` | `object` | Safety flags (no binary editing, etc.). |
| `targetFiles` | `array` | List of files to be modified. |
| `operations` | `array` | List of discrete edit operations. |
| `blockedFiles` | `array` | Files explicitly prohibited in this patch. |
| `deferredFiles` | `array` | Files deferred to later phases. |
| `validationPlan` | `object` | Checklist for pre/post-edit and in-game checks. |
| `rollbackPlan` | `object` | Steps to restore state on failure. |
| `unsupportedClaims` | `array` | Claims that need in-game verification. |

## First Patch Scope (Timecycle)

Allowed candidates for `first_patch`:
- `visualsettings.dat` (named key candidate)
- `cloudkeyframes.xml` (color-like candidate)
- `timecycle_mods_1.xml` (color-like candidate)

Blocked/Deferred:
- `weather.xml` (Deferred until weather files validated)
- `timecycle_mods_3.xml` (Blocked until parameter mapper exists)
- Any `.rpf`, `.ytd`, `.yft` (Blocked — no binary editing)

## Operations

Operations must use specific tools:
- `dat_named_key_editor`
- `xml_cloudkeyframe_editor`
- `xml_timecycle_editor`
- `xml_weather_editor`

All directions (e.g., "darken sky") must be treated as **hypotheses** unless directly proven by scanner evidence.
