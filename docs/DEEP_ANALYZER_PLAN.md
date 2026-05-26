# Deep Analyzer Plan

The scanner core tells us which files changed. Deep analyzers explain what changed inside each file format.

## Analyzer 1 — XML / Timecycle

Priority: first.

Files:

```text
timecycle_mods_1.xml
timecycle_mods_2.xml
timecycle_mods_3.xml
timecycle_mods_4.xml
w_clear.xml
w_clouds.xml
w_extrasunny.xml
w_foggy.xml
w_halloween.xml
w_neutral.xml
```

Tasks:

- parse XML
- compare clean vs modded values
- group changed values by file and node path
- detect kill effect/timecycle_mods_4 changes
- generate AI-friendly JSON/Markdown

## Analyzer 2 — DAT/META/YMT

Priority: second.

Files:

```text
bloodfx.dat
visualsettings.dat
*.meta
*.ymt
```

Tasks:

- text/line diff
- key/value diff where possible
- flag risky changes

## Analyzer 3 — YTD Texture Dictionary

Priority: third.

Tasks:

- list texture names
- dimensions
- formats
- mipmaps
- metadata comparison
- detect added/removed textures
- optional preview/export later

## Analyzer 4 — GFX/SWF

Priority: fourth.

Tasks:

- decompile/convert GFX/SWF
- list sprites/shapes/assets
- detect color changes
- detect embedded image changes

## Analyzer 5 — YPT Particle

Priority: hardest.

Tasks:

- list particle effects
- identify tracer/hit-effect candidates
- compare particle parameters
- detect removed effects/references

Do not start with YPT. Start with XML/timecycle.
