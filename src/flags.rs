//! Atomic flags are surprisingly difficult to design well. Too many
//! and the code gets complex; too few and the code gets
//! complex. Complexity also means slowness in the case of this
//! library which is supposed to be a simple primitive and thus we
//! want to make it efficient.
//!
//! The rules:
//!
//! * Every time you change the flags, you must check the other half
//!   did not drop.
//!   * If it did, you must clean up the channel.
//!   * This includes setting Dropped flags.
//! * Only a Receiver may set ReceiverDropped or ReceiverWaker
//! * Only a Sender may set SenderDropped or SenderWaker
//! * Before reading or writing a waker, the lock must be obtained
//! * After reading or writing a waker, the lock must be returned
//!   unless a Dropped flag was set.
//! * After you have set a Dropped flag, if you were not beaten to it,
//!   you must not touch the channel again.

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Flag {
    /// The Receiver has dropped. The sender will become responsible
    /// for freeing the channel if it isn't already.
    ReceiverDropped = 1,
    /// The Sender has dropped. The receiver will become responsible
    /// for freeing the channel if it isn't already.
    SenderDropped   = 1 << 1,

    #[cfg(feature="async")]
    /// Exclusive access to the waitings
    Locked  = 1 << 2,
    #[cfg(feature="async")]
    /// The Receiver's waiting is locked.
    ReceiverWaker = 1 << 3,
    #[cfg(feature="async")]
    /// The Sender's waiting has been set.
    SenderWaker   = 1 << 4,
}

pub trait Test<T> : Sized {
    fn all(self, flags: T)  -> bool;
    fn any(self, flags: T)  -> bool;
    fn none(self, flags: T) -> bool;
}

impl Test<u8> for Flag {
    #[inline(always)]
    fn all(self, flags: u8) -> bool { (flags & self as u8) != 0 }
    #[inline(always)]
    fn any(self, flags: u8) -> bool { (flags & self as u8) != 0 }
    #[inline(always)]
    fn none(self, flags: u8) -> bool { (flags & self as u8) == 0 }
}

impl Test<Flags> for Flag {
    #[inline(always)]
    fn all(self, flags: Flags) -> bool { (flags.0 & self as u8) != 0 }
    #[inline(always)]
    fn any(self, flags: Flags) -> bool { (flags.0 & self as u8) != 0 }
    #[inline(always)]
    fn none(self, flags: Flags) -> bool { (flags.0 & self as u8) == 0 }
}

#[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Flags(u8);

impl Test<u8> for Flags {
    #[inline(always)]
    fn all(self, flags: u8) -> bool { (flags & self.0) == flags }
    #[inline(always)]
    fn any(self, flags: u8) -> bool { (flags & self.0) != 0 }
    #[inline(always)]
    fn none(self, flags: u8) -> bool { (flags & self.0) == 0 }
}

impl Test<Flags> for Flags {
    #[inline(always)]
    fn all(self, flags: Flags) -> bool { (flags.0 & self.0) == flags.0 }
    #[inline(always)]
    fn any(self, flags: Flags) -> bool { (flags.0 & self.0) != 0 }
    #[inline(always)]
    fn none(self, flags: Flags) -> bool { (flags.0 & self.0) == 0 }
}

impl<T: Into<Flags>> BitAnd<T> for Flags {
    type Output = Flags;
    #[inline(always)] fn bitand(self, other: T)  -> Flags { Flags(self.0 & other.into()) }
}
impl<T: Into<Flags>> BitAnd<T> for Flag {
    type Output = Flags;
    #[inline(always)] fn bitand(self, other: T)  -> Flags { Flags(self as u8 & other.into()) }
}
impl BitAnd<Flags> for u8 {
    type Output = u8;
    #[inline(always)] fn bitand(self, other: Flags)  -> u8 { self & other.0 }
}
impl BitAnd<Flag> for u8 {
    type Output = u8;
    #[inline(always)] fn bitand(self, other: Flag)  -> u8 { other as u8 & self }
}
impl BitAndAssign for Flags {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = Self(self.0 & rhs.0)
    }
}

impl<T: Into<Flags>> BitOr<T> for Flags {
    type Output = Flags;
    #[inline(always)] fn bitor(self, other: T)  -> Flags { Flags(self.0 | other.into()) }
}
impl<T: Into<Flags>> BitOr<T> for Flag {
    type Output = Flags;
    #[inline(always)] fn bitor(self, other: T)  -> Flags { Flags(self as u8 | other.into()) }
}
impl BitOr<Flags> for u8 {
    type Output = u8;
    #[inline(always)] fn bitor(self, other: Flags)  -> u8 { self | other.0 }
}
impl BitOr<Flag> for u8 {
    type Output = u8;
    #[inline(always)] fn bitor(self, other: Flag)  -> u8 { other as u8 | self }
}
impl BitOrAssign for Flags {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = Self(self.0 | rhs.0)
    }
}

impl<T: Into<Flags>> BitXor<T> for Flags {
    type Output = Flags;
    #[inline(always)] fn bitxor(self, other: T)  -> Flags { Flags(self.0 ^ other.into()) }
}
impl<T: Into<Flags>> BitXor<T> for Flag {
    type Output = Flags;
    #[inline(always)] fn bitxor(self, other: T)  -> Flags { Flags(self as u8 ^ other.into()) }
}
impl BitXor<Flags> for u8 {
    type Output = u8;
    #[inline(always)] fn bitxor(self, other: Flags)  -> u8 { self ^ other.0 }
}
impl BitXor<Flag> for u8 {
    type Output = u8;
    #[inline(always)] fn bitxor(self, other: Flag)  -> u8 { other as u8 ^ self }
}
impl BitXorAssign for Flags {
    fn bitxor_assign(&mut self, rhs: Self) {
        *self = Self(self.0 ^ rhs.0)
    }
}

impl From<Flag> for Flags {
    #[inline(always)] fn from(flag: Flag) -> Flags { Flags(flag as u8) }
}
impl From<Flag> for u8 {
    #[inline(always)] fn from(flag: Flag) -> u8 { flag as u8 }
}
impl From<Flags> for u8 {
    #[inline(always)] fn from(flags: Flags) -> u8 { flags.0 }
}

impl Not for Flag {
    type Output = u8;
    #[inline(always)] fn not(self) -> u8 { !(self as u8) }
}
impl Not for Flags {
    type Output = u8;
    #[inline(always)] fn not(self) -> u8 { !self.0 }
}
