// Copyright 2022 Andrew D. Straw.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Associates an encoder with a writer to allow writing encoded frames.

use std::io::Write;

use super::{Error, LessEncoder, Result, YCbCrImage};

/// An encoding session ready to start but which has not yet necessarily encoded
/// its first frame.
///
/// This mainly exists to hold the writer but defer writing until we have the
/// first frame (in the `Configured` variant). After the first frame is written,
/// it will be in the `Recording` variant. (The `MovedOut` variant should never
/// be observed and represents a temporary internal state.)
enum WriteState<W> {
    Configured(W),
    Recording(RecordingState<W>),
    MovedOut,
}

impl<W: Write> WriteState<W> {
    fn write_frame(&mut self, frame: &YCbCrImage) -> Result<()> {
        // Temporarily replace ourself with a dummy value.
        let orig_state = std::mem::replace(self, WriteState::MovedOut);
        let state = match orig_state {
            WriteState::Configured(fd) => {
                let (initial_nal_data, encoder) = LessEncoder::new(frame)?;
                let mut state = RecordingState { wtr: fd, encoder };
                state
                    .wtr
                    .write_all(&initial_nal_data.sps.to_annex_b_data())?;
                state
                    .wtr
                    .write_all(&initial_nal_data.pps.to_annex_b_data())?;
                state
                    .wtr
                    .write_all(&initial_nal_data.frame.to_annex_b_data())?;
                state
            }
            WriteState::Recording(mut state) => {
                let encoded = state.encoder.encode(frame)?;
                state.wtr.write_all(&encoded.to_annex_b_data())?;
                state
            }
            WriteState::MovedOut => {
                return Err(Error::InconsistentState {
                    #[cfg(feature = "backtrace")]
                    backtrace: std::backtrace::Backtrace::capture(),
                })
            }
        };

        // Restore ourself to the correct state.
        *self = WriteState::Recording(state);

        Ok(())
    }
}

/// Small helper struct holding writer and encoder for an ongoing encoding
/// session.
struct RecordingState<W> {
    wtr: W,
    encoder: LessEncoder,
}

/// Write images to an [std::io::Write] implementation in `.h264` file format.
pub struct H264Writer<W> {
    inner: WriteState<W>,
}

impl<W: Write> H264Writer<W> {
    /// Create a new [H264Writer] from an [std::io::Write] implementation.
    pub fn new(wtr: W) -> Result<Self> {
        Ok(Self {
            inner: WriteState::Configured(wtr),
        })
    }

    /// Retrieve the underlying [std::io::Write] implementation.
    pub fn into_inner(self) -> W {
        match self.inner {
            WriteState::Configured(w) => w,
            WriteState::Recording(state) => state.wtr,
            WriteState::MovedOut => {
                unreachable!("inconsistent internal state");
            }
        }
    }

    /// Encode and write a frame
    pub fn write(&mut self, frame: &YCbCrImage) -> Result<()> {
        self.inner.write_frame(frame)
    }
}
