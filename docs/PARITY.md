# Parity checklist

What the reference demo (26.1.7, installed 2026-11-10) contains, as a to-do list. Names on the left are theirs and are used here only to track coverage; our own parts get our own names.

## Instruments (51)
- [~] 3x Osc (built, approximate)
- [ ] Autogun
- [ ] BassDrum
- [ ] BeepMap
- [~] BooBass: a plucked string model stands in for the sampled bass; the three tone knobs are read. Tone and decay are estimates.
- [ ] Dashboard
- [~] DirectWave: the playing program's zones (keys, velocities, root key, file, loop points) are read and played across the keyboard. Zone envelopes, filters, LFOs, tuning and level settings, the effects section and program switching are not done; a short fade stands in for the release.
- [ ] Drumaxx
- [ ] Drumpad
- [ ] FLEX
- [~] FL Keys: plays the user's own sample sets (the documented set description files), all fifteen settings read; built tones stand in when no set is found. The stereo ambience, the exact treble filter and the overdrive curve are estimates.
- [ ] FL Slayer
- [ ] FL Studio Mobile
- [ ] FL Studio Mobile Rack
- [~] FPC: pads, their velocity layers, levels, pans, layer tune, cut groups and output offsets are read and played, with a pad page to audition them. Pad envelopes, the scale-volume and random-layer switches, the built-in pattern loops and editing are not done. The tune knob's reach is an estimate.
- [~] Fruit Kick: all six knobs read; the end pitch is exact (taken from the step the block stores), the start pitch base, sweep and decay times, click and distortion scales are estimates.
- [ ] Fruity Dance
- [ ] Fruity DrumSynth Live
- [~] Fruity DX10: follows the public two-operator design it descends from; the second modulator's settings are read in an assumed order and share the first one's touch setting.
- [ ] Fruity Envelope Controller
- [ ] Fruity Granulizer
- [ ] Fruity Keyboard Controller
- [~] Fruity Slicer: the loop and its cuts are read and each key plays its slice at the channel pitch. The pitch and time stretch sliders, reversed slices and the slice view are not done.
- [ ] Fruity Slicer 2
- [ ] Fruity Video Player
- [ ] Fruity Voltage Controller
- [ ] Fruity Wrapper
- [ ] GMS
- [~] Harmless (built as Harmonic Stack, approximate; its built-in chorus, delay, reverb and compressor, harmonic detune, grit, slides and velocity links are not run yet)
- [~] Harmor (built as Harmonic Forge on the additive engine: timbre mix, unison, the lower filter, pluck, phaser and harmonizer of the first layer are read at estimated scales; the second layer, picture resynthesis, drawn envelopes, filter types, built-in effects and a fresh block for new instances are not done, so it arrives with projects and presets only)
- [ ] Kepler
- [ ] Kepler Exo
- [ ] MIDI Out
- [ ] Minisynth
- [ ] Morphine
- [ ] Ogun
- [ ] Patcher
- [ ] Plucked!
- [ ] PoiZone
- [ ] ReWired
- [ ] Sakura
- [ ] Sawer
- [ ] SimSynth
- [~] Slicex: the loop (kept in the block or named by path) and its marker points are read, and each key from C5 plays its slice. Decks, articulators, envelopes, filters, the wave editor and region tools are not done.
- [ ] Soundfont Player
- [~] Sytrus (built as Six Op, approximate: operators, modulation matrix, three filters with shapers, envelopes, LFOs, mappings, unison; state layouts 17 to 25 are read, older ones 5 to 15 are not yet; its effects section and arpeggio breaks are not run, the panel is read-only and it cannot yet be added fresh from the menu)
- [ ] Toxic Biohazard
- [ ] Transistor Bass
- [ ] Wasp
- [ ] Wasp XT
- [ ] Wave Traveller

## Effects and tools (90)
- [ ] Control Surface
- [ ] Distructor
- [ ] Edison
- [~] Effector (built as Pad Effects; all twelve effects are our own takes, tuned by ear from the manual text)
- [~] Emphasis (built as Finisher: gain rider plus shaper, four modes, hardness, linking, ceiling; a plain stand-in for the adaptive stages, switches and meters missing)
- [ ] Emphasizer
- [~] EQUO (built as Morph Equaliser: 31 bands, eight curves with morph, master curve, per-band pan, shift, smoothing; mix knob not located in the block, band gain law and shift range estimated, sends and curve drawing missing)
- [ ] FL Studio Mobile Rack FX
- [~] Frequency Shifter (built as Hertz Shifter: shift, ring mode, sidebands per side, feedback, phase offset, width; the frequency law is fitted to one known point, the tempo range and the side input are estimates)
- [~] Frequency Splitter (built as Band Splitter; main output only, band sends not run)
- [ ] Fruity 7 Band EQ
- [~] Fruity Balance (built, approximate)
- [~] Fruity Bass Boost (built as Bass Lift; frequency and amount scales estimated)
- [ ] Fruity Big Clock
- [~] Fruity Blood Overdrive (built, approximate)
- [ ] Fruity Center
- [~] Fruity Chorus (built, approximate)
- [~] Fruity Compressor (built, approximate)
- [ ] Fruity Convolver
- [ ] Fruity dB Meter
- [ ] Fruity Delay
- [~] Fruity Delay 2 (built, approximate)
- [~] Fruity Delay 3 (built, approximate)
- [~] Fruity Delay Bank (built as Echo Bank; grain, shelf filters and filter gains not run)
- [~] Fruity Fast Dist (built, approximate)
- [~] Fruity Fast LP (built, approximate)
- [~] Fruity Filter (built, approximate)
- [~] Fruity Flanger (built as Flanger (classic), settings carried onto the newer unit)
- [~] Fruity Flangus (built, approximate)
- [ ] Fruity Formula Controller
- [~] Fruity Free Filter (built as Open Filter: seven shapes; frequency law fitted to three known points, Q and gain scales estimated)
- [ ] Fruity HTML NoteBook
- [~] Fruity Limiter (built, approximate)
- [~] Fruity Love Philter (built as Filter Bank: eight banks in parallel or in a row, twelve filter types, slopes, curve shaper, pattern 1 of each target, pad and input-follower mappings; packed block read and written. Not yet: slow oscillators, patterns 2 to 10 and key switching, line editors, the note text. Filter laws, resonance scale and follower times are estimates; the type order is inferred from named presets)
- [ ] Fruity LSD
- [~] Fruity Multiband Compressor (built as Three Band Compressor, approximate)
- [ ] Fruity Mute 2
- [ ] Fruity NoteBook
- [ ] Fruity NoteBook 2
- [~] Fruity PanOMatic (built, approximate)
- [~] Fruity Parametric EQ (built as Seven Band EQ (classic), settings carried onto the newer unit)
- [~] Fruity Parametric EQ 2 (built, approximate)
- [~] Fruity Peak Controller (built as Level Follower; drives mixer levels, slot mix and effect settings; link mapping formulas not run)
- [ ] Fruity Phase Inverter
- [~] Fruity Phaser (built as Phaser: up to 23 stages, stereo sweep, feedback; depth to frequency and output gain scales estimated)
- [~] Fruity Reeverb (built as Room Reverb (classic), settings carried onto the newer unit)
- [~] Fruity Reeverb 2 (built, approximate)
- [ ] Fruity Scratcher
- [ ] Fruity Send
- [~] Fruity Soft Clipper (built, approximate)
- [ ] Fruity Spectroman
- [ ] Fruity Squeeze
- [~] Fruity Stereo Enhancer (built, approximate)
- [~] Fruity Stereo Shaper (built, approximate)
- [ ] Fruity Vocoder
- [~] Fruity WaveShaper (built as Curve Shaper; drawn line read, no editor for it yet)
- [ ] Fruity Wrapper
- [~] Fruity X-Y Controller (built as Pad Controller; outputs its stored position, no pad window yet)
- [ ] Fruity X-Y-Z Controller
- [~] Gross Beat (built as Beat Juggler: two-bar ring, 36 time and 36 volume lines read from the block, mixes, attack and release; no line editor yet, no scratch dial, hold button or key switching of slots; follows its own bar count from the start of play; attack and release times are estimates)
- [ ] Hardcore
- [ ] Hyper Chorus
- [~] Low Lifter (built as Low Harmonics: band split, overtone curve, gains, mix; which stored value is which knob is partly guessed, diffusion missing)
- [ ] Luxeverb
- [~] LuxeVerb (built as Plush Reverb on the Room Reverb tank; pitch loop, follower, freeze and peak filter not run)
- [~] Maximus (built as Band Maximiser; drawn lines read, no editor, look-ahead and second state not run)
- [ ] Multiband Delay
- [ ] Newtime
- [ ] Newtone
- [ ] Patcher
- [~] Pitcher (built as Voice Tuner; automatic correction only, note-driven modes, harmony voices, formant and gender not run)
- [~] Pitch Shifter (built as Grain Shifter; formant setting not run)
- [ ] Razer Chroma
- [~] Soundgoodizer (built, approximate)
- [ ] Spreader
- [ ] Transient Processor
- [ ] Transmitter
- [ ] Transporter
- [ ] Tuner
- [ ] VFX Color Mapper
- [ ] VFX Envelope
- [ ] VFX Keyboard Splitter
- [ ] VFX Key Mapper
- [ ] VFX Level Scaler
- [ ] VFX Script
- [ ] VFX Sequencer
- [~] Vintage Chorus (built as Bucket Chorus; noise not run)
- [ ] Vintage Phaser
- [ ] Vocodex
- [ ] Wave Candy
- [ ] ZGameEditor Visualizer

## Core windows
- [~] Channel rack (steps, channel strip, layers, add sampler or instrument)
- [~] Piano roll (draw, move, resize, delete; tools and lanes still to do)
- [~] Playlist (pattern, audio and automation clips; tools still to do)
- [~] Mixer (inserts, sends, routing, EQ, effect slots; docking and metering details still to do)
- [~] Browser (folders, preview, drag to channel)
- [~] Transport, toolbars, hint bar
- [~] Automation clips (channel levels and pitch, mixer levels, slot mix and switch, effect settings; instrument settings still to do)
- [ ] Event editor
- [ ] Audio recording and audio editor
- [ ] Plugin hosting (VST/CLAP)
- [~] Project files: .flp read/write done; .fst presets, .fsc scores to do
- [~] Keyboard and mouse bindings (docs/KEYBINDINGS.md)
- [ ] MIDI devices, controller linking
- [~] Rendering/export: WAV done; MP3, OGG, FLAC, MIDI to do

## What "approximate" means
`[~]` marks a part that works but has not been measured against the reference. Until measured, treat these as
close, not identical:
- Level laws: channel volume, mixer fader, velocity, pan law, send levels.
- Sampler: envelope curve shapes and tension, filter cutoff and resonance scales, filter types, time stretch
  quality, echo spacing, portamento time.
- Track EQ frequency and width scales.
- Every effect unit: how stored values map to decibels, milliseconds, hertz and percentages. The stored layout
  was worked out by comparing many project files, so a few settings may be attached to the wrong knob. The
  least certain: Flanger (eight stored values against twelve knobs), Echo Lab (feedback, diffusion and
  modulation positions), Chorus wave selectors, Limiter time laws, Reverb character, Compressor knee types.
- Harmonic Stack: which stored field is which control was inferred from value ranges and the public manual; every
  time, depth and frequency scale, the built-in timbre list and the menu orders are estimates to be measured.
  Band Maximiser: the drawn line is read as linear level with its middle as full level; attack, release and
  sustain time laws, the low cut scale, both saturation shapes and the separation direction are estimates.
  Three Band Compressor: band order, cutoff scale, threshold, ratio and knee scales are estimates.
  Bucket Chorus: which stored fields are high pass, gain and noise, the time and speed scales.
  Grain Shifter: meaning of density and colour, grain sizes.
  Band Splitter: slopes above 24 dB an octave are capped.
  Echo Bank: feedback mode numbering, cutoff and resonance scales, offset units.
  Pad Effects: rate switch positions, the multiplier range, every effect's ranges.
  Voice Tuner: speed law, lowest pitch law, note switch order (taken as C first).
  Level Follower: output order (both, peak, sweep), decay and speed laws, tension curve, which switch byte lets the sound through.
  Effect settings under automation: the setting number is taken as our listed order and the travel as our listed range.
  Plush Reverb: which stored fields are size, diffusion, brightness and character, the decay law
  (0.1 s to 100 s across the travel), the gain law (746 is unity, twenty units to the dB), which switch byte is
  the predelay tempo lock.
- Six Op: modulation depth, envelope time unit, LFO speed scale, knob laws, filter type order (worked out from
  statistics over the user's presets), curve tension sense, shaper curve reading, the level against other
  instruments. The effects send column is treated as plain output until the effects section exists.
- Triple Osc: field order inside an oscillator block, the mix law between oscillators, detune width, the
  meaning of the four switches.
- Instruments and effects not listed as built play a stand-in voice or pass sound through. Their settings are
  kept and saved untouched.
