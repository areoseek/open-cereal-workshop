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
