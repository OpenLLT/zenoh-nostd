#![cfg_attr(not(feature = "std"), no_std)]

pub const VERSION: u8 = 9;

pub mod logging;

pub mod zerror;
pub(crate) use zerror::*;

mod bytes;
pub use bytes::*;

mod codec;
pub(crate) use codec::*;

mod ke;
pub use ke::*;

pub mod msgs;
pub use msgs::{exts, fields};

mod transport;
pub use transport::*;

pub(crate) use zenoh_derive::*;

#[cfg(test)]
mod tests;

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ZInstant(core::time::Duration);

impl From<core::time::Duration> for ZInstant {
    fn from(value: core::time::Duration) -> Self {
        Self(value)
    }
}

impl From<ZInstant> for core::time::Duration {
    fn from(value: ZInstant) -> Self {
        value.0
    }
}
