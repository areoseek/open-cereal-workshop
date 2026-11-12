# Keyboard and mouse bindings

Parity goal: every binding an experienced user relies on does the same thing here. The tables describe behaviour
in our own words. "yes" means wired in this build, "part" means it works with a noted difference, "no" means
still to do.

Keys go to the window that was clicked last (or reached with Tab); the bindings under "Everywhere" work whichever
window that is, unless that window has its own use for the key.

**Number pad.** The window toolkit hands us the number pad's digits, plus, minus and slash as the same keys as
the ones on the main block, so the two cannot be told apart yet. Where the original uses the number pad for one
thing and the top row for another, the top-row meaning wins while the channel rack has the keys or the typing
keyboard is on, and the number pad meaning applies otherwise.

## Everywhere

### Files and history
| Binding | Action | Done |
|---|---|---|
| Ctrl+O | Open a project | yes |
| Ctrl+S | Save | yes |
| Ctrl+Shift+S | Save as | yes |
| Ctrl+N | Save a new version (name_2, name_3 ...) | yes |
| Alt+1 ... Alt+9, Alt+0 | Open one of the ten projects used last | yes |
| Ctrl+R | Export audio (WAV by default) | yes |
| Ctrl+Shift+R | Export audio as MP3 | yes |
| Ctrl+Shift+M | Export a MIDI file | yes |
| Ctrl+Z | Undo the last edit, or redo it if it was just undone | yes |
| Ctrl+Alt+Z | Step back through the history | yes |

### Transport and recording
| Binding | Action | Done |
|---|---|---|
| Space | Play / stop | yes |
| Ctrl+Space | Play / pause | yes |
| L | Pattern mode / song mode | yes |
| R | Recording on / off | yes |
| Number pad 0 | Jump forward a bar (hold to keep going) | part: with the typing keyboard off |
| Number pad / and * | Previous / next bar | yes |
| Ctrl+B | Blend recorded notes with the ones already there, or replace them | yes |
| Ctrl+E | Step editing: notes played while stopped are written one grid step at a time | yes |
| Ctrl+I | Wait for the first note before starting | yes |
| Ctrl+M | Metronome | yes |
| Ctrl+P | Countdown before recording | yes |
| Ctrl+T | Typing keyboard to piano | yes |
| Ctrl+H | Stop all sound | yes |
| Z S X D C ... and Q 2 W 3 E ... | Notes from the typing keyboard, two rows | yes |
| Backspace | Snap: none, and back to what it was (piano roll and playlist) | yes |

### Patterns
| Binding | Action | Done |
|---|---|---|
| Number pad 1 ... 9 | Select pattern 1 ... 9 | part: with the typing keyboard off |
| Number pad + / - | Next / previous pattern | yes |
| F4 | Next empty pattern, and name it | yes |
| Ctrl+F4 | Next empty pattern | yes |
| F2 | Rename the pattern | part: repeated presses do not walk the colours |
| Ctrl+Shift+C | Clone the pattern | yes |

### Windows
| Binding | Action | Done |
|---|---|---|
| F5 / F6 / F7 / F9 | Playlist / channel rack / piano roll / mixer | yes |
| F8 | Plug-in picker | yes |
| Alt+F8 | Browser | yes |
| F10 | Settings | yes |
| F11 | Project information | yes |
| F12 | Close all windows | yes |
| F1 | Help (the About window for now) | part |
| Tab / Shift+Tab | On to the next / previous open window | yes |
| Esc | Close the window that has the keys | yes |
| Enter | Let the playlist fill the work area, and back | yes |
| Ctrl+Shift+H | Put the windows back where they start | yes |
| F3 | Tool selector | no |

## Channel rack
| Binding | Action | Done |
|---|---|---|
| 1 ... 9, 0 | Mute / unmute one of the first ten channels | part: with the typing keyboard off |
| Ctrl+1 ... Ctrl+0 | Solo one of the first ten channels | yes |
| Up / Down | Select the channel above / below | yes |
| Alt+Up / Alt+Down | Move the selected channels up / down | yes |
| Alt+C | Clone the selected channels | yes |
| Alt+Del | Delete the selected channels | yes |
| Alt+M | Show / hide the mixer track boxes | yes |
| Ctrl+L | Send each selected channel to a free mixer track | yes |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste the channel's steps and notes | yes |
| Shift+Ctrl+Left / Right | Turn the steps round by one step | yes |
| Ctrl+Enter | Go to the playlist | yes |
| F2 | Rename the channel | yes |
| Click / right-click a step | Set / clear | yes |
| Click / right-click the light | Mute / solo | yes |
| Click the channel name | Select it, open or close its settings | yes |
| Drop a sample, preset or score on a name | Load it | yes |
| Shift+Up / Shift+Down | Add to the selection | yes |
| Click, Shift+click, right-click the selector beside a name | Select that channel alone, add or drop it, select all on show | yes |
| Alt+G | Put the selected channels in a new or existing group, by name | yes |
| Alt+Z / Alt+U | Zip the selected channels to a thin strip / unzip | yes |
| PgUp / PgDn | Next / previous channel group | yes |
| Ctrl+Alt+R | Restretch all audio clips | no |
| Shift+wheel on a channel name | Move that channel up / down | yes |

## Mixer
| Binding | Action | Done |
|---|---|---|
| Left / Right | Select the track to the left / right | yes |
| S | Solo the track | part: with the typing keyboard off |
| Alt+S | Solo the track together with what feeds it and what it feeds | yes |
| Alt+Left / Alt+Right | Move the track | yes |
| Ctrl+L | Send the selected channels to this track | yes |
| Shift+Ctrl+L | Send the selected channels to this track and the ones after it, one each | yes |
| Alt+L | Select the channel that plays into this track | yes |
| F2 | Rename the track | yes |
| Ctrl+Shift+S | Save the track's state | yes |
| Ctrl+A / Ctrl+D | Select all the tracks / only the one with the keys | yes |
| Ctrl+click, Shift+click a name plate, Shift+Left / Right | Add to the selection, select the run up to it; the selected tracks' faders, pans and mutes move together | yes |
| Alt+R | Render the armed tracks to audio files | no |
| Alt+W | Wave view | no |
| Shift+wheel on a track's name plate | Move that track left / right | yes |

## Piano roll
| Binding | Action | Done |
|---|---|---|
| P, B, D, T, C, E, Z, Y | Draw, paint, delete, mute, slice, select, zoom, playback tool | yes (typing keyboard off) |
| S / O | Slide notes / portamento notes | yes |
| F / Shift+F | Next / previous property in the lane under the notes | yes |
| G / K | Channel above / below | yes |
| H / J | Channel above / below that has notes | yes |
| Shift+H / Shift+K | The same | yes |
| Ctrl+A / Ctrl+D | Select all / deselect | yes |
| Shift+I | Invert the selection | yes |
| Shift+C | Select every note of the selected notes' colours | yes |
| Shift+R | Select notes by chance | yes |
| Ctrl+Left / Ctrl+Right | Select the stretch of time before / after the selection | yes |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste | yes |
| Ctrl+B | Duplicate to the right | yes |
| Del | Delete the selection | yes |
| Ctrl+Ins / Ctrl+Del | Open a gap as long as the selection / close the selection's time up | yes |
| Shift+Left / Right | Move by the snap value | yes |
| Alt+Left / Right | Nudge by one tick | yes |
| Shift+Up / Down, Ctrl+Up / Down | Transpose by a semitone / an octave | yes |
| Ctrl+Q, Ctrl+L, Ctrl+U, Ctrl+G | Quick quantise, legato, chop, glue | yes |
| Shift+D | Discard note lengths | yes |
| Alt+C | Give the notes the colour being drawn with | yes |
| Alt+M / Shift+Alt+M | Mute / unmute | yes |
| Alt+V | Ghost notes of the other channels | yes |
| Alt+E, Q, U, A, S, F, W, K, Y, R, X, O, L | Riff machine, quantise, chop, arpeggiate, strum, flam, claw, limit, flip, randomise, scale levels, LFO, articulate | yes |
| Ctrl+M | Import a MIDI file or score | yes |
| PgUp / PgDn | Zoom in / out | yes |
| Shift+0 | Centre the view on the play position | yes |
| Shift+1 / 2 / 3 | One, four, sixteen bars across the view | yes |
| Shift+4 | Show the whole pattern | yes |
| Shift+5 | Zoom on the selection | yes |
| Backspace | Snap off and back | yes |
| Alt held | No snap | yes |
| Ctrl+drag, Shift+drag a note | Rectangle select, drag a copy | yes |
| Double-click a note | Note properties | yes |
| Wheel, Shift+wheel, Ctrl+wheel, middle drag | Scroll, sideways, zoom, pan | yes |
| M | Keyboard / note names view | yes |
| Alt+B, Alt+N | Note helpers, waveform helper | no |
| Alt+G / Shift+G | Ungroup / group the selected notes; a group is picked, moved and copied as one | yes |
| Ctrl+I | Write the lane's current value | no |
| Ctrl+Enter | Select the time round the selection | yes |
| Ctrl+drag on the ruler | Pick out a stretch of time; it plays round and round, and Ctrl+Ins, Ctrl+Del, Ctrl+Left and Ctrl+Right work on it; Ctrl+D lets go of it | yes |
| Shift+Ctrl+V | Paste from the MIDI clipboard | no |
| Shift+M, Shift+N | Random groups, chord stamp off | no |
| Alt+wheel | Change the property of the note under the pointer (and the selection it belongs to) | yes |
| Right Shift+click | Slice, and its variants | no |

## Playlist
| Binding | Action | Done |
|---|---|---|
| P, B, D, T, S, C, E, Z, Y | Draw, paint, delete, mute, slip, slice, select, zoom, playback tool | yes (typing keyboard off) |
| Ctrl+A / Ctrl+D | Select all / deselect | yes |
| Shift+I | Invert the selection | yes |
| Shift+C | Select every clip that plays the same source | yes |
| Ctrl+Left / Ctrl+Right | Select the stretch of time before / after the selection | yes |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste | yes |
| Ctrl+B | Duplicate to the right | yes |
| Del | Delete the selection | yes |
| Ctrl+Ins / Ctrl+Del | Open a gap as long as the selection / close the selection's time up | yes |
| Shift+arrows | Move by the snap value, or a track up or down | yes |
| Shift+Q | Pull clip starts onto the grid | yes |
| Alt+M / Shift+Alt+M | Mute / unmute | yes |
| Home | Play position to the start | yes |
| Alt+T | Add a time marker at the play position and name it | yes |
| Alt+* / Alt+/ | Play position to the next / previous time marker | yes |
| Click, drag, right-click a marker | Jump to it, move it, rename or delete it | part: no marker action menu yet |
| PgUp / PgDn | Zoom in / out | yes |
| 5, Shift+5 | Zoom on the selection | part: plain 5 with the typing keyboard off |
| Shift+0 | Centre the view on the play position | yes |
| Shift+1 / 2 / 3 | Four, sixteen, sixty-four bars across the view | yes |
| Shift+4 | Show the whole song | yes |
| Backspace | Snap off and back | yes |
| Alt held | No snap | yes |
| Ctrl+drag, Shift+drag a clip, clip edges | Rectangle select, drag a copy, resize | yes |
| Right-click a clip | Delete | yes |
| Wheel, Shift+wheel, Ctrl+wheel, middle drag | Scroll, sideways, zoom, pan | yes |
| 6, Shift+6, Ctrl+P | Performance mode and its zoom | no |
| 7 / 8, 9 | Transpose an audio clip down / up, reverse it | no |
| Insert | Slice the clip under the pointer at the grid line | yes |
| Shift+Alt+T | Time signature change | no |
| Shift+G / Alt+G | Group / ungroup the selected clips; a group is picked, moved and copied as one | yes |
| Ctrl+Alt+C, Ctrl+Alt+Shift+C | Consolidate the selection | no |
| Ctrl+Alt+G | Grid colour | no |
| Ctrl+Enter | Select the time round the selection | yes |
| Ctrl+drag on the ruler | Pick out a stretch of time; it plays round and round, and Ctrl+Ins, Ctrl+Del, Ctrl+Left and Ctrl+Right work on it; Ctrl+D lets go of it | yes |
| Ctrl+F8, Alt+P | Project picker, picker panel | no |
| Shift+M | Stretch mode | no |
| Shift+wheel on a track name | Move the track up / down | yes |
| Shift+Alt+wheel | Nudge the clip under the pointer by the snap value | yes |
| Right Shift, right Alt clicks | Slice, mute and quantise by clicking | no |

## Browser
| Binding | Action | Done |
|---|---|---|
| Click | Preview a sound | yes |
| Drag onto a channel, the rack, the playlist or a mixer slot | Load it there | yes |
| Enter, Ctrl+Alt+click | Replace the selected channel with the item | yes |
| Up / Down, Shift+Up / Down | Step through the files on show, playing each | yes |
