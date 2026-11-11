# Breakfast Cereal DAW

An open source, pattern-based music workstation written in Rust. Native on Linux, also built for Windows.

> [!WARNING]
> **Bring your own FL Studio sounds. We cannot include them.**
> This project ships none of FL Studio's samples, presets or demo songs, and never will: they belong to
> Image-Line. On first start the program offers to read them from **your own installed copy** (the free trial
> you download yourself from Image-Line counts). You point it at the installed folder, or at the installer you
> downloaded so it can install itself. The files are read where they sit on your disk; nothing is copied into
> this repository, bundled with releases or uploaded. Getting that copy and following its licence is up to
> you. You can skip the step ("Not now" asks again next start, "Do not ask again" stops asking) and work with
> the built-in instruments and your own sample folders. It is always available under Options.

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
