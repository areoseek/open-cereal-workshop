# Open Cereal Workshop

An open source, pattern-based music workstation written in Rust. Native on Linux, also built for Windows.

> [!WARNING]
> **You must bring your own FL Studio installer file. We cannot include it, or anything from it.**
> This project ships none of FL Studio's samples, presets or demo songs, and never will: they belong to
> Image-Line. On first start the program asks for one thing: the FL Studio installer file, which **you download
> yourself** from Image-Line (the free trial is the same file). The program reads the sounds, presets, demo
> projects and templates out of that file into its own data folder on your disk. Nothing is installed or run,
> nothing is uploaded, and none of it is in this repository or in any release. Getting that file and
> following Image-Line's licence for what is in it is up to you. You can skip the step ("Not now" asks again
> next start, "Do not ask again" stops asking) and work with the built-in instruments and your own sample
> folders. It is always available under Options.

    cargo run --release                      # the application
    cargo run --release -- song.flp          # open a project

On Linux, `packaging/install.sh` puts the built program, a menu entry and the icon under `~/.local`. `packaging/appimage.sh` makes a single-file AppImage (it needs appimagetool), `packaging/PKGBUILD` an Arch package, and `packaging/windows.iss` is the Inno Setup script for a Windows installer. The build workflow makes the AppImage and the installer too; neither has been through a real release yet.

It opens and saves `.flp` project files. A project that is opened and saved without changes comes back byte for
byte; edits are patched into the original file so settings we do not understand yet are kept.

Current state
- The windows: channel rack with step sequencer and graph editor, piano roll with its tools, playlist with
  patterns, audio clips and automation clips, mixer (inserts, sends, routing, track EQ, ten effect slots),
  browser, event editor, channel settings, project information, settings.
- The keys and mouse moves of the reference program (docs/KEYBINDINGS.md).
- Instruments and effects: every stock plug-in a project can name has a unit of our own that reads the same
  stored settings, under a name of our own. How close each one is, and what is still an estimate, is listed
  per unit in docs/PARITY.md.
- Other makers' plug-ins: CLAP, VST3 and VST2 hosting, and projects that name an installed plug-in open with its saved sound.
- Audio and MIDI recording, MIDI keyboards and controllers with learned links, MIDI output, a built-in audio
  editor.
- Export to WAV, FLAC, MP3, Ogg Vorbis and MIDI, whole song, pattern, time selection or one file per mixer
  track.
- Autosave into a backup folder, and a question before unsaved work is thrown away.

See docs/ROADMAP.md for what is left, docs/PARITY.md for coverage and for what is still approximate, and
docs/GROUND-RULES.md for how the work is done.
