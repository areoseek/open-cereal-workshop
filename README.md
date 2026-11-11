# Breakfast Cereal DAW

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

It opens and saves `.flp` project files. A project that is opened and saved without changes comes back byte for
byte; edits are patched into the original file so settings we do not understand yet are kept.

Current state
- Channel rack with step sequencer, piano roll, playlist, mixer (inserts, sends, routing, track EQ), browser.
- Sampler channels with envelopes, filter, time stretch, echo; layers; audio clips; automation clips.
- Built-in instrument: Triple Osc. Other generators play through a plain stand-in voice and keep their settings.
- Eighteen built-in effect units (EQ, limiter, reverb, compressor, delays, chorus, flanger, filters, distortion,
  stereo tools). Other effects pass sound through and keep their settings.
- WAV export, offline render example (`cargo run --release --example render -- in.flp out.wav`).

See docs/ROADMAP.md for what is left, docs/PARITY.md for coverage and for what is still approximate, and
docs/GROUND-RULES.md for how the work is done.
