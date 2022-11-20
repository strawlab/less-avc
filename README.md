# less-avc - Less Advcanced Video Coding - H.264 Encoder

[![Crates.io](https://img.shields.io/crates/v/less-avc.svg)](https://crates.io/crates/less-avc)
[![Documentation](https://docs.rs/less-avc/badge.svg)](https://docs.rs/less-avc/)
[![Crate License](https://img.shields.io/crates/l/less-avc.svg)](https://crates.io/crates/less-avc)

This module contains a pure Rust implementation of an H.264 encoder optimized
for lossless encoding. It is simple ("less advanced") and uses a small subset of
the encoder features in the H.264 specification.

Features and characteristics:
- Pure rust.
- Lossless encoding of 8 bit and 12 bit monochrome (luminance only 4:0:0) and
  color (4:2:0) data.
- Includes an optimized path for luminance-only data in which no chroma data is
  saved.
- Encodes every frame as an I (intra) frame (also "keyframe") using PCM
  encoding.
- Tests decode image with [`openh264`](https://crates.io/crates/openh264) and
  [ffmpeg](https://ffmpeg.org) to ensure encoded image is losslessly preserved.

Desired but not implemented feature:
 - Support for other bit-depths and chroma sampling resolutions (e.g. 4:4:4).

Worthy of consideration features:
 - Support for context-adaptive variable-length coding (CAVLC).
 - Support for context-adaptive binary arithmetic coding (CABAC).

This was inspired by Ben Mesander's [World's Smallest H.264
Encoder](https://www.cardinalpeak.com/blog/worlds-smallest-h-264-encoder).

## Testing

Run the basic tests with:

```
cargo test
```

Full round-trip tests with ffmpeg and openh264 are in the `testbench` directory
and crate. For those:

```
cd testbench
cargo test
```

These tests can export the created streams to `.h264` files if the
`LESSAVC_SAVE_TEST_H264` environment variable is set. To convert these to
`.mp4`:

```
#!/bin/bash -x
set -o errexit

FILES="./*.h264"
for f in $FILES
do
    echo "Processing $f file..."
    ffmpeg -i $f -vcodec copy $f.mp4
    # ffmpeg -i $f $f.png
done
```

## Benchmarking

Benchmarks are in the `testbench` directory and crate:

```
cd testbench
cargo bench

# Or, to benckmark while compiling for the native CPU architecture, like so:
RUSTFLAGS='-C target-cpu=native' cargo bench
```

## License

Copyright 2022 Andrew D. Straw.

Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
http://opensource.org/licenses/MIT>, at your option.
