# Roadmap

Done
- Engine: lock-free song hand-off to the audio thread, 256 voices, sample playback (WAV/FLAC and friends), sampler
  envelopes, filter, portamento, cut groups, echo, time stretch, layers, audio clips, automation clips.
- Project files: `.flp` read and write. Unchanged projects save byte-exact (checked against 165 reference
  projects); edits are patched into the original event stream.
- Windows: channel rack, piano roll, playlist, mixer, browser, channel settings, effect editors.
- Mixer: inserts, sends, routing, track EQ, stereo separation, ten effect slots per insert with enable and mix.
- Effects (our own units reading the stored settings): Balance, Soft Clip, Quick Distortion, Stereo Tool,
  Loudener, Quick Low-pass, Overdrive, Filter, Auto Pan, Delay, Compressor, Stereo Matrix, Seven Band EQ,
  Room Reverb, Limiter, Flanger, Chorus, Echo Lab.
- Instruments: Triple Osc.
- WAV export and offline render.

Next, roughly in order
- Piano roll and playlist: the full tool set (draw, paint, delete, mute, slice, select, zoom, playback), selection
  editing, clipboard, quantise, note properties lane, with the reference key and mouse bindings.
- More instruments by how often projects use them: FM synth, additive synth, drum pads, slicer, sample player.
- More effects: multiband dynamics, wave shaper, transient shaper, phaser, pitch tools, convolution, vocoder.
- Automation of effect and instrument settings; channel LFOs; arpeggiator; time shift; loop points.
- Audio and MIDI recording, MIDI devices and controller linking.
- Plugin hosting (CLAP first, then VST3/LV2).
- Windows build in CI; free, properly licensed starter sounds (docs/CONTENT-SOURCES.md).
- Measuring every "approximate" item in docs/PARITY.md against the reference and correcting it.
