# Keyboard and mouse bindings

Parity goal: every binding an experienced user relies on behaves the same. Listed as behaviour, never as copied
manual text. "yes" means wired in this build.

## Global
| Binding | Action | Done |
|---|---|---|
| Space | Play / stop | yes |
| L | Switch pattern / song mode | yes |
| Ctrl+Space | Play / pause | yes |
| Ctrl+N / Ctrl+O | New / open project | yes |
| Ctrl+S / Ctrl+Shift+S | Save / save as | yes |
| Ctrl+R | Export to WAV | yes |
| Ctrl+Z / Ctrl+Alt+Z | Undo-redo last step / step back through history | yes |
| Ctrl+T | Typing keyboard to piano on / off | yes |
| Ctrl+M | Metronome | yes |
| Ctrl+H | Stop all sound | yes |
| Ctrl+Shift+C | Clone pattern | yes |
| F2 | Rename pattern | yes |
| F4 | New pattern | yes |
| F5 / F6 / F7 / F9 | Playlist / channel rack / piano roll / mixer | yes |
| Alt+F8 | Browser | yes |
| F11 / F12 | Project info / close all windows | yes |
| + / - | Next / previous pattern | yes |
| Z S X D C ... and Q 2 W 3 E ... | Play notes from the typing keyboard (two rows, two octaves) | yes |
| Ctrl+L, Alt+L etc. | Channel routing and other menu shortcuts | no |

## Channel rack
| Binding | Action | Done |
|---|---|---|
| Click / right-click a step | Set / clear | yes |
| Click the channel name | Select, open channel settings | yes |
| Drop a sample on a channel | Load it | yes |
| Alt+Up / Alt+Down | Move channel | no |
| Shift+Ctrl+Left / Right | Shift steps | no |

## Piano roll and playlist
See the tables in the source next to each tool as they are wired (app/src/pianoroll.rs, app/src/playlist.rs).
Targets: tool keys P (draw), B (paint), D (delete), T (mute), C (slice), E (select), Z (zoom), Y (playback);
Ctrl+A/C/X/V/B, Shift+arrows nudges, Ctrl+Up/Down octave, Alt+snap off, Ctrl+Q quick quantise, Shift+wheel and
Ctrl+wheel scrolling and zoom.

## Piano roll (when it was the last window clicked)

| Keys | Action |
|---|---|
| P, B, D, T, C, E, Z, Y | Draw, Paint, Delete, Mute, Slice, Select, Zoom, Playback tool (typing keyboard off) |
| Ctrl+A / Ctrl+D | Select all / deselect |
| Ctrl+C, Ctrl+X, Ctrl+V | Copy, cut, paste (paste lands on the first bar in view) |
| Ctrl+B | Duplicate to the right |
| Delete | Delete the selection |
| Shift+Left / Right | Nudge by the snap value |
| Shift+Up / Down, Ctrl+Up / Down | Transpose by a semitone, by an octave |
| Ctrl+Q, Ctrl+L, Ctrl+U, Ctrl+G | Quick quantise, legato, chop, glue |
| Alt+M / Shift+Alt+M | Mute / unmute |
| Ctrl+drag, Shift+drag a note, Alt held | Rectangle select, drag a copy, no snap |
| Wheel, Shift+wheel, Ctrl+wheel, middle drag | Scroll, scroll sideways, zoom, pan |
