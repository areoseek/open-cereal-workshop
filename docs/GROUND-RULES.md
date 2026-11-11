# Ground rules: how this project stays our own work

Breakfast Cereal DAW is an independent program that aims to do what a pattern-based DAW does. It is not a copy of FL Studio.

Allowed sources
- Using the public demo the way any user would, and writing down what each control does.
- The public manual, public tutorials, and general DSP and audio literature.
- Open standards and formats: WAV, FLAC, MIDI, SoundFont, CLAP, LV2, VST3 (under its licence).

Never
- Decompiling, disassembling, or debugging the original binaries, or reading its memory.
- Copying any shipped content: samples, loops, presets, demo songs, impulse responses, wavetables.
- Copying artwork, icons, skins, fonts, layouts pixel for pixel, or manual text.
- Using the names "FL Studio", "Fruity", "Image-Line" or their plugin names for our program or its parts
  (they may be mentioned only to state compatibility, such as "imports .flp" if we ever do that from public information).

Every sound and preset shipped here is made by us (synthesized or recorded) or comes from a source whose licence allows redistribution, recorded in docs/CONTENT-SOURCES.md when the first one is added.

Feature notes go in docs/features/ as plain descriptions of behaviour ("the step button toggles a note at that 16th"), never as copied text or screenshots of the original.

## Compatibility target (Devin, 2026-11-10)

An experienced FL Studio user must be able to sit down and work from muscle memory.
- Same behaviour and workflow for every feature, same keyboard and mouse bindings, same window roles and layout logic.
- Reads and writes .flp projects and the related preset/score formats. The format is worked out only from files we save ourselves with the demo and from public format notes; never from the program's code.
- Our own drawing of the interface: same places and same behaviour, our own artwork, icons and colours.
- Stock sounds are ours. A project that points at the original's stock samples plays them only from the user's own installed copy.
