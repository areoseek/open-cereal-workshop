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
- [ ] Sytrus
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
- [ ] Frequency Splitter
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
- [ ] Fruity Delay Bank
- [~] Fruity Fast Dist (built, approximate)
- [~] Fruity Fast LP (built, approximate)
- [~] Fruity Filter (built, approximate)
- [ ] Fruity Flanger
- [~] Fruity Flangus (built, approximate)
- [ ] Fruity Formula Controller
- [ ] Fruity Free Filter
- [ ] Fruity HTML NoteBook
- [~] Fruity Limiter (built, approximate)
- [ ] Fruity Love Philter
- [ ] Fruity LSD
- [ ] Fruity Multiband Compressor
- [ ] Fruity Mute 2
- [ ] Fruity NoteBook
- [ ] Fruity NoteBook 2
- [~] Fruity PanOMatic (built, approximate)
- [ ] Fruity Parametric EQ
- [~] Fruity Parametric EQ 2 (built, approximate)
- [ ] Fruity Peak Controller
- [ ] Fruity Phase Inverter
- [ ] Fruity Phaser
- [ ] Fruity Reeverb
- [~] Fruity Reeverb 2 (built, approximate)
- [ ] Fruity Scratcher
- [ ] Fruity Send
- [~] Fruity Soft Clipper (built, approximate)
- [ ] Fruity Spectroman
- [ ] Fruity Squeeze
- [~] Fruity Stereo Enhancer (built, approximate)
- [~] Fruity Stereo Shaper (built, approximate)
- [ ] Fruity Vocoder
- [ ] Fruity WaveShaper
- [ ] Fruity Wrapper
- [ ] Fruity X-Y Controller
- [ ] Fruity X-Y-Z Controller
- [ ] Gross Beat
- [ ] Hardcore
- [ ] Hyper Chorus
- [ ] Low Lifter
- [ ] Luxeverb
- [ ] Maximus
- [ ] Multiband Delay
- [ ] Newtime
- [ ] Newtone
- [ ] Patcher
- [ ] Pitcher
- [ ] Pitch Shifter
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
- [ ] Vintage Chorus
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
- Triple Osc: field order inside an oscillator block, the mix law between oscillators, detune width, the
  meaning of the four switches.
- Instruments and effects not listed as built play a stand-in voice or pass sound through. Their settings are
  kept and saved untouched.
