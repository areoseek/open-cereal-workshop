# Third-party code

Parts of this project come from other open projects. Their notices are kept here and in the files themselves.

## WavPack (reader in `engine/src/wavpack.rs`)

The WavPack reader is a Rust port of the decoding path of the WavPack reference library, used under the BSD
3-clause licence below.

    Copyright (c) 1998 - 2025 David Bryant
    All rights reserved.

    Redistribution and use in source and binary forms, with or without
    modification, are permitted provided that the following conditions are met:

        * Redistributions of source code must retain the above copyright notice,
          this list of conditions and the following disclaimer.
        * Redistributions in binary form must reproduce the above copyright notice,
          this list of conditions and the following disclaimer in the
          documentation and/or other materials provided with the distribution.
        * Neither the name of the copyright holder nor the names of its
          contributors may be used to endorse or promote products derived from
          this software without specific prior written permission.

    THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
    AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
    IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
    ARE DISCLAIMED. IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE FOR
    ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
    DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
    SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
    CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
    OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
    OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## rusty_mp3 and rusty_vorbis (MP3 and Ogg Vorbis export)

The MP3 and Ogg Vorbis writers in `engine/src/lossy.rs` use the `rusty_mp3` and `rusty_vorbis` crates, pure
Rust encoders by Mata Network (https://github.com/Remade-With-Rust/remade_ffmpeg_rs), used unmodified as
dependencies under the Apache License, Version 2.0. The full licence text ships inside each crate and is at
https://www.apache.org/licenses/LICENSE-2.0.

## CLAP (plug-in hosting)

`engine/src/host/abi.rs` and the test plug-in in `testplug/` follow the layout of the CLAP plug-in standard
(https://github.com/free-audio/clap), written out in Rust from its public headers. The standard is under the
MIT License:

    MIT License

    Copyright (c) 2021 Alexandre BIQUE

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the "Software"), to deal
    in the Software without restriction, including without limitation the rights
    to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software, and to permit persons to whom the Software is
    furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in all
    copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    SOFTWARE.

## VST3 (plug-in hosting)

`engine/src/host/abi3.rs` and the VST3 face of the test plug-in in `testplug/` follow the layout of the VST3
plug-in interfaces (https://github.com/steinbergmedia/vst3_pluginterfaces), written out in Rust from their
public headers. No Steinberg code is included. The interfaces are under the MIT License:

    MIT License

    Copyright (c) 2026, Steinberg Media Technologies GmbH

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the "Software"), to deal
    in the Software without restriction, including without limitation the rights
    to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software, and to permit persons to whom the Software is
    furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in all
    copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    SOFTWARE.

VST is a trademark of Steinberg Media Technologies GmbH. This project is not affiliated with or endorsed by
Steinberg.

## libloading (opening plug-in files)

The `libloading` crate by Simonas Kazlauskas opens plug-in files at run time. It is used unmodified as a
dependency under the ISC License; the licence text ships inside the crate.

## x11rb (windows for plug-in editors on Linux)

The `x11rb` crate by Uli Schlachter and contributors makes the X11 window a VST3 plug-in's editor is shown
in. It is used unmodified as a dependency under the MIT License or the Apache License 2.0, at our choice;
the licence texts ship inside the crate.

## Open-Unmix (stem separation, `engine/src/stems.rs`)

Extract stems from sample runs the Open-Unmix networks by Fabian-Robert Stöter, Stefan Uhlich, Antoine Liutkus
and Yuki Mitsufuji ("Open-Unmix - A Reference Implementation for Music Source Separation", Journal of Open
Source Software, 2019). The code that reads the weight files and runs the networks is our own, written from
the published description. The UMX-HQ weight files (trained on MUSDB18-HQ) are not part of this repository
or of any release: the program downloads them from their publication record on zenodo.org (record 3370489)
the first time stems are asked for, and keeps them in its data folder. The weights are published under the
MIT License:

    MIT License

    Copyright (c) 2019 Inria (Fabian-Robert Stöter, Antoine Liutkus)

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the "Software"), to deal
    in the Software without restriction, including without limitation the rights
    to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software, and to permit persons to whom the Software is
    furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in all
    copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    SOFTWARE.
