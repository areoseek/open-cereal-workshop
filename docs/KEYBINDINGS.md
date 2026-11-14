# Keyboard and mouse bindings

Parity goal: every binding an experienced user relies on does the same thing here. The tables describe behaviour
in our own words. "yes" means wired in this build, "part" means it works with a noted difference, "no" means
still to do.

Keys go to the window that was clicked last (or reached with Tab); the bindings under "Everywhere" work whichever
window that is, unless that window has its own use for the key.

**Number pad.** Number pad keys are told apart from the top row (and the right Shift, Ctrl and Alt keys from
the left ones) by a small addition of ours to the window toolkit, kept in `vendor/egui-winit`. On a keyboard
without a number pad the top row does the number pad's job wherever it has no use of its own: with the typing
keyboard off and outside the channel rack.

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
| Number pad 0 | Jump forward a bar (hold to keep going) | yes |
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
| Number pad 1 ... 9 | Select pattern 1 ... 9 | yes |
| Number pad + / - | Next / previous pattern | yes |
| Shift+F4 | First empty pattern, and name it | yes |
| F4 | Next empty pattern, and name it | yes |
| Ctrl+F4 | Next empty pattern, no naming | yes |
| Shift+Ctrl+Ins | Insert an empty pattern here | yes |
| Shift+Ctrl+Del | Delete the pattern | yes |
| Shift+Ctrl+P | Show the pattern in the browser's project tab | yes |
| Shift+Ctrl+Up / Down | Move the current pattern up / down the list (outside the piano roll and playlist) | yes |
| F2 | Rename the pattern | yes |
| Ctrl+Shift+C | Clone the pattern | yes |

### Windows
| Binding | Action | Done |
|---|---|---|
| Ctrl+F11 | Hide and show the toolbars | yes |
| Shift+F12 | Align all channel editors | yes |
| Shift+E (mixer) | Open the audio editor on the track, armed to record | yes |
| Del (mixer) | Delete the selected mixer tracks | yes |
| Alt+R / Shift+Alt+R (mixer) | Render the armed / the selected tracks to wave files | yes |
| F5 / F6 / F7 / F9 | Playlist / channel rack / piano roll / mixer | yes |
| F8 | Plug-in picker | yes |
| Alt+F8 | Browser | yes |
| F10 | Settings | yes |
| F11 | Project information | yes |
| F12 | Close all windows | yes |
| Alt+F12 | Close all plug-in windows | yes |
| Ctrl+F12 | Close every window but the one in use | yes |
| Shift+Ctrl+H | Put the windows back in their places | yes |
| Ctrl+F8 | Project picker | yes |
| Shift+F8 | Browser on its plug-in tab | yes |
| Ctrl+F5 | Tempo tapper | yes |
| Alt+F7 | Touch controller | yes |
| Ctrl+F | Find in the browser, from any window | yes |
| F1 | Help: the bindings of the window in use, with a find box | yes |
| Tab / Shift+Tab | On to the next / previous open window | yes |
| Esc | Close the window that has the keys | yes |
| Enter | Let the playlist fill the work area, and back | yes |
| Ctrl+Shift+H | Put the windows back where they start | yes |
| F3 | Tool selector (the tools of the focused window, at the pointer) | yes |

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
| Shift+Alt+C / Ctrl+Alt+C | Colour the selected channels as a gradient from the first to the last / at random | yes |
| PgUp / PgDn | Next / previous channel group | yes |
| Ctrl+Alt+R | Restretch all audio clips (stretching already follows the tempo live; this redoes the treated copies) | yes |
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
| Alt+R | Render the selected tracks to audio files, one each, in the project's Recorded folder | yes |
| Alt+W | Wave view: the meters show each track's latest sound as a wave | yes |
| Ctrl+E | Open the audio editor on the track | yes |
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
| Double click on a ghost note | Switch to that note's channel (Editable ghosts) | yes |
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
| Alt+B, Alt+N | Note helpers (the light rows), waveform helper (the channel sample behind the notes) | yes |
| Alt+G / Shift+G | Ungroup / group the selected notes; a group is picked, moved and copied as one | yes |
| Shift+Ctrl+Left / Right | Rotate the selected notes inside the stretch they cover, one snap step | yes |
| Shift+O | Select every other note | yes |
| Ctrl+Alt+V | Editable ghosts | yes |
| Alt+P | Generate chord progression | yes |
| Ctrl+I | Write the control's current value (with the event editor open: an event at the play position) | yes |
| Alt+O (event editor) | LFO tool | yes |
| Alt+X (event editor) | Scale levels tool | yes |
| Ctrl+X / Ctrl+C / Ctrl+V / Del (event editor) | Cut, copy, paste, delete the shown control's events | yes |
| Ctrl+Enter | Select the time round the selection | yes |
| Ctrl+drag on the ruler | Pick out a stretch of time; it plays round and round, and Ctrl+Ins, Ctrl+Del, Ctrl+Left and Ctrl+Right work on it; Ctrl+D lets go of it | yes |
| Shift+Ctrl+V | Paste from the MIDI clipboard | yes (the clipboard holds the path of a MIDI file) |
| Shift+M, Shift+N | Select groups of notes at random; chord stamp off (the Stamp menu on the toolbar picks a chord the draw tool lays down) | yes |
| Alt+wheel | Change the property of the note under the pointer (and the selection it belongs to) | yes |
| Right Shift+click, right Alt+click, right Alt+right-click | Slice; mute; quick quantise | yes (with the right button the shorter piece of each cut note is dropped) |
| Ctrl+T | Add a time marker at the play position, named by itself | yes |
| Shift+Alt+T | Add a time signature change | yes |
| Ctrl+Alt+Ins | Slice the notes at the selection's start and open a gap as long as the selection | yes |
| Ctrl+Alt+Home | Resize notes from their left edge | yes |
| Ctrl+Alt+Y | Run the last helper tool again | yes |
| Shift+Enter | Flip the separator (control lane above the notes) | yes |
| Shift+6 | Step sequencer zoom | yes |
| Shift+wheel over a note | Nudge it by the snap value (Edit > wheel nudge) | yes |

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
| Click, drag, right-click a marker | Jump to it, move it, rename or delete it | yes |
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
| Ctrl+Alt+Ins | Slice the clips at the start of the selection and open a gap there | yes |
| Ctrl+Alt+G | Merge the selected automation clips | yes |
| Ctrl+Alt+Home | Allow resizing clips from their left edge, on and off | yes |
| Shift+Ctrl+B | Clone the tracks of the selected clips | yes |
| Ctrl+T / Shift+T (playlist focused) | Add an "Auto" marker / place a loop marker | yes |
| Double click on a clip | Open it the way "Edit patterns in" says | yes |
| Ctrl+P (playlist focused), Shift+6 | Performance mode on and off, fit the performance zone | yes |
| Click a zone clip; right-click it or click the track name | Start the clip on its grid; stop the track | yes |
| Alt+click, Shift+click, Ctrl+click a zone clip | Start at once, queue after the playing clip, start the whole column | yes |
| Click a marker tab over the zone | Play that scene | yes |
| Typing keys while performing | Rows 1-5, QWERT, ASDFG, ZXCVB, 6-0, YUIOP, HJKL;, NM,./ start clips 1 to 5 of tracks 1 to 8 (layout to be checked) | partly |
| 7 / 8, 9 | Transpose an audio clip down / up, reverse it | yes |
| Insert | Slice the clip under the pointer at the grid line | yes |
| Shift+Alt+T | Time signature change at the start of the picked time or on the current bar | yes |
| Shift+G / Alt+G | Group / ungroup the selected clips; a group is picked, moved and copied as one | yes |
| Ctrl+Alt+C, Ctrl+Alt+Shift+C | Consolidate the selected clips into one audio clip (from the song start with Shift); the clips are muted under it | yes |
| Ctrl+G / Shift+Ctrl+G | Merge the selected pattern clips into one new pattern / merge clips of the same pattern on the same track | yes |
| Shift+Ctrl+V | Make unique: each selected clip gets its own copy of its pattern or channel | yes |
| Ctrl+Enter | Select the time round the selection | yes |
| Ctrl+drag on the ruler | Pick out a stretch of time; it plays round and round, and Ctrl+Ins, Ctrl+Del, Ctrl+Left and Ctrl+Right work on it; Ctrl+D lets go of it | yes |
| Alt+P | Picker panel: the patterns, audio clips and automation clips at the left of the playlist; a click picks what is drawn next | yes |
| Ctrl+F8 | Project picker | yes |
| Shift+M | Stretch mode: dragging an audio clip's end fits the sound to the new length, pitch kept | yes |
| Shift+wheel on a track name | Move the track up / down | yes |
| Shift+Alt+wheel | Nudge the clip under the pointer by the snap value | yes |
| Right Shift+click, right Alt+click, right Alt+right-click | Slice; mute; hear an audio clip | yes (with the right button the shorter piece of the cut clip is dropped) |

## Browser
| Binding | Action | Done |
|---|---|---|
| Click | Preview a sound | yes |
| Drag onto a channel, the rack, the playlist or a mixer slot | Load it there | yes |
| Enter, Ctrl+Alt+click | Replace the selected channel with the item | yes |
| Up / Down | Step through the files on show, playing each | yes |
| Shift+Up / Shift+Down | Send the previous / next file to the selected channel | yes |
| Middle-click | Send the file to the selected channel (a project opens) | yes |
| Ctrl+F | Find | yes |
| F2 / F3 | Previous / next search result | yes |
| Ctrl+Up | Collapse the folder structure | yes |
| Ctrl+R | Reread the folder structure | yes |
| Click a folder's row | Open or shut it | yes |
| Star at the end of a row, right-click > Favorite | Mark as a favourite (Starred page) | yes |
| Drag a plug-in from the plug-in page | Onto the rack: a new channel; onto a mixer track or slot: the effect goes there | yes |
| Backspace / Shift+Backspace | Navigate back / forward | yes |
| Click a sound; click the wave strip at the foot of the browser | Play it whole; play it from the place clicked (right-click stops it) | yes |

## Wave Recorder (audio editor)

These work while the pointer is over the wave picture.

| Key | Action | Done |
|---|---|---|
| Ctrl+O / Ctrl+S | Open a sound / save the marked stretch or everything | yes |
| Ctrl+Z | Undo | yes |
| Ctrl+X / Ctrl+C / Ctrl+V | Cut / copy / paste in at the cursor | yes |
| Alt+V / Shift+V | Paste over what is there / paste mixed in | yes |
| Del / Ctrl+Del | Delete the marked stretch / keep only it | yes |
| Shift+Del | Silence the marked stretch | yes |
| Alt+Del | Delete everything after the loop end | yes |
| Ins | Insert silence | yes |
| Alt+A | Amplify | yes |
| Ctrl+N | Normalize | yes |
| Ctrl+F / Alt+F | Fade in / fade out | yes |
| Alt+Left | Reverse | yes |
| Alt+N | Take the noise level from the marked stretch | yes |
| Alt+U / Ctrl+U | Learn the noise from the marked stretch / clean it out | yes |
| Alt+S | Slice at the hits | yes |
| M / Alt+M | Add a marker / make the marked stretch a region | yes |
| Alt+L | Make the marked stretch the loop | yes |
| Shift+Ctrl+Del | Delete markers, regions and loop in the marked stretch | yes |
| Left / Right | Previous / next region | yes |
| Up | Let go of the marked stretch | yes |
| Ctrl+A | Mark everything | yes |
| PgUp / PgDn | Zoom in / out | yes |
| Home / End | Show everything / show the marked stretch | yes |
| Ctrl+Left / Ctrl+Right | Bring the start / end of the marked stretch into view | yes |
| Shift+C | Send the marked stretch to the playlist | yes |
| Alt+T / Ctrl+T | Time stretch and pitch shift / drum stretch | yes |
| Alt+W | Claw machine: throw away every so-manyth stretch of beats | yes |
| Ctrl+R / Ctrl+B / Ctrl+Q | Reverb / blur / equalise | yes |
| Ctrl+L | Tune the loop | yes |
| S | Frequencies in place of the wave | yes |
