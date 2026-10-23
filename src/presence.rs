//! Compile-time discovery of optional hooks, without executing the hook.
use core::marker::PhantomData;

/// The result of a possibly absent customization hook.
///
/// Ordinary values implement this trait with `PRESENT = true`. Default hooks
/// return an internal marker with `PRESENT = false`. Only extract a value after
/// checking presence. This trait is a safe protocol: consumers must not use the
/// flag to justify unchecked memory access.
pub trait Presence<R = ()> {
    /// Whether this return type represents an implemented hook.
    const PRESENT: bool;
    /// Extract the value. An absent implementation panics.
    fn into_value(self) -> R;
}

impl<R> Presence<R> for R {
    const PRESENT: bool = true;
    #[inline]
    fn into_value(self) -> R {
        self
    }
}

#[derive(Debug)]
pub(crate) struct Missing<R>(PhantomData<fn() -> R>);

pub(crate) const fn missing<R>() -> Missing<R> {
    Missing(PhantomData)
}

impl<R> Presence<R> for Missing<R> {
    const PRESENT: bool = false;
    fn into_value(self) -> R {
        panic!("optional fast_io hook is not provided")
    }
}

/// Detect a hook taking one argument (usually `&self`).
#[inline]
pub const fn provided<A, R, P: Presence<R>>(_: impl FnOnce(A) -> P + Copy) -> bool {
    P::PRESENT
}

/// Detect a hook taking no arguments.
#[inline]
pub const fn provided0<R, P: Presence<R>>(_: impl FnOnce() -> P + Copy) -> bool {
    P::PRESENT
}

/// Detect a hook taking two arguments, including the receiver.
#[inline]
pub const fn provided2<A, B, R, P: Presence<R>>(_: impl FnOnce(A, B) -> P + Copy) -> bool {
    P::PRESENT
}

/// Detect a hook taking three arguments, including the receiver.
#[inline]
pub const fn provided3<A, B, C, R, P: Presence<R>>(_: impl FnOnce(A, B, C) -> P + Copy) -> bool {
    P::PRESENT
}
