# Roadmap

Done (2026-11-10)
- M0: workspace, lock-free engine, 16-step channel rack, four synthesized drums, transport, tempo, master. Linux build.

Next, in order
- M1 (in progress): project model DONE, save/load in our .cereal format DONE, WAV export DONE. Left: per-step velocity, sample playback channel (WAV/FLAC), Windows build in CI, a real file-open dialog.
- M2: piano roll, a first subtractive synth, note properties (velocity, pan, pitch).
- M3: playlist (arrange patterns and audio clips on tracks), song/pattern mode.
- M4: mixer (inserts, sends, master), first effects: EQ, compressor, reverb, delay, limiter, filter.
- M5: automation clips, MIDI input, recording.
- FLP import (in progress): the .flp event reader (engine/src/flp.rs) parses header, version, title, tempo (old format), channel names and pattern notes from real demo projects; `flp2cereal` example converts a file to our format. Remaining: tempo in the newest (25.2+) format, per-note piano-roll import (not just step folding), playlist/arrangement, plugin/channel state, and live import into the running app (needs a variable channel set behind the audio stream).
- M6: plugin hosting (CLAP first, then VST3/LV2).
- M7 onward: the long tail of instruments and effects, one at a time, each written from its own behaviour notes.
