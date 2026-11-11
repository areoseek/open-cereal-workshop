# Parity checklist

What the reference demo (26.1.7, installed 2026-11-10) contains, as a to-do list. Names on the left are theirs and are used here only to track coverage; our own parts get our own names.

## Instruments (51)
- [~] 3x Osc (built, approximate)
- [ ] Autogun
- [ ] BassDrum
- [ ] BeepMap
- [ ] BooBass
- [ ] Dashboard
- [ ] DirectWave
- [ ] Drumaxx
- [ ] Drumpad
- [ ] FLEX
- [ ] FL Keys
- [ ] FL Slayer
- [ ] FL Studio Mobile
- [ ] FL Studio Mobile Rack
- [ ] FPC
- [ ] Fruit Kick
- [ ] Fruity Dance
- [ ] Fruity DrumSynth Live
- [ ] Fruity DX10
- [ ] Fruity Envelope Controller
- [ ] Fruity Granulizer
- [ ] Fruity Keyboard Controller
- [ ] Fruity Slicer
- [ ] Fruity Slicer 2
- [ ] Fruity Video Player
- [ ] Fruity Voltage Controller
- [ ] Fruity Wrapper
- [ ] GMS
- [~] Harmless (built as Harmonic Stack, approximate; its built-in chorus, delay, reverb and compressor, harmonic detune, grit, slides and velocity links are not run yet)
- [ ] Harmor
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
- [ ] Slicex
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
- [ ] Effector
- [ ] Emphasis
- [ ] Emphasizer
- [ ] EQUO
- [ ] FL Studio Mobile Rack FX
- [ ] Frequency Shifter
- [~] Frequency Splitter (built as Band Splitter; main output only, band sends not run)
- [ ] Fruity 7 Band EQ
- [~] Fruity Balance (built, approximate)
- [ ] Fruity Bass Boost
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
- [ ] Fruity Free Filter
- [ ] Fruity HTML NoteBook
- [~] Fruity Limiter (built, approximate)
- [ ] Fruity Love Philter
- [ ] Fruity LSD
- [~] Fruity Multiband Compressor (built as Three Band Compressor, approximate)
- [ ] Fruity Mute 2
- [ ] Fruity NoteBook
- [ ] Fruity NoteBook 2
- [~] Fruity PanOMatic (built, approximate)
- [~] Fruity Parametric EQ (built as Seven Band EQ (classic), settings carried onto the newer unit)
- [~] Fruity Parametric EQ 2 (built, approximate)
- [ ] Fruity Peak Controller
- [ ] Fruity Phase Inverter
- [ ] Fruity Phaser
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
- [ ] Fruity X-Y Controller
- [ ] Fruity X-Y-Z Controller
- [ ] Gross Beat
- [ ] Hardcore
- [ ] Hyper Chorus
- [ ] Low Lifter
- [ ] Luxeverb
- [~] LuxeVerb (built as Plush Reverb on the Room Reverb tank; pitch loop, follower, freeze and peak filter not run)
- [~] Maximus (built as Band Maximiser; drawn lines read, no editor, look-ahead and second state not run)
- [ ] Multiband Delay
- [ ] Newtime
- [ ] Newtone
- [ ] Patcher
- [ ] Pitcher
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
- [~] Automation clips (channel and mixer levels; effect and instrument settings still to do)
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
