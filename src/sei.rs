// Copyright 2022-2023 Andrew D. Straw.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Supplemental Enhancement Information (SEI) encoding

use alloc::{vec, vec::Vec};

use super::RbspData;

/// User data unregistered [SupplementalEnhancementInformation] message
#[derive(Debug, PartialEq, Eq)]
pub struct UserDataUnregistered {
    pub uuid: [u8; 16],
    pub payload: Vec<u8>,
}

impl UserDataUnregistered {
    pub fn new(uuid: [u8; 16], payload: Vec<u8>) -> Self {
        Self { uuid, payload }
    }
    fn to_sei_payload(&self) -> Vec<u8> {
        let mut result = self.uuid.to_vec();
        result.extend(self.payload.clone());
        result
    }
}

/// Supplemental Enhancement Information
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SupplementalEnhancementInformation {
    /// User data unregistered message
    UserDataUnregistered(UserDataUnregistered),
}

impl SupplementalEnhancementInformation {
    /// Encode into raw byte sequence payload
    pub fn to_rbsp(&self) -> RbspData {
        let (payload_type, payload) = match &self {
            Self::UserDataUnregistered(udr) => (5u8, udr.to_sei_payload()),
        };
        let mut payload_size = payload.len();
        let mut num_ff_bytes = 0;
        while payload_size > 255 {
            num_ff_bytes += 1;
            payload_size -= 0xff;
        }
        let mut result = vec![0xff; num_ff_bytes + 2];
        let size_idx = result.len() - 1;
        result[0] = payload_type;
        result[size_idx] = payload_size.try_into().unwrap();

        result.extend(payload);
        result.push(0x80); // rbsp_trailing_bits
        RbspData { data: result }
    }
}
