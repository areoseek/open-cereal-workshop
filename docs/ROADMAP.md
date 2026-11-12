# Roadmap

Done
- Engine: lock-free song hand-off to the audio thread, sampler, layers, audio clips, automation clips,
  plug-in delay compensation, offline render.
- Project files: `.flp` read and write. Unchanged projects save byte-exact (checked against the reference
  projects a user's own copy brings); edits are patched into the original event stream.
- Windows: channel rack, piano roll, playlist, mixer, browser, event editor, channel settings, effect and
  instrument editors, audio editor, settings, project information.
- A unit of our own for every stock instrument and effect, each reading the stored settings.
- CLAP and VST3 hosting.
- Audio and MIDI recording, MIDI devices, controller linking, MIDI output and sync.
- Export: WAV, FLAC, MP3, Ogg Vorbis, MIDI; time selection, tail handling, one file per mixer track.
- First-start import of sounds, presets, demo projects and templates from the user's own installer file.
- Autosave and backups.
- Builds and tests on Linux and Windows (.github/workflows/build.yml).

Left, roughly in order
- Measuring every item marked as an estimate in docs/PARITY.md against the reference and correcting it.
- The "Not done" notes in docs/PARITY.md, unit by unit.
- Free, properly licensed starter sounds (docs/CONTENT-SOURCES.md).
- Packages: an AppImage or Flatpak for Linux, an installer for Windows.
