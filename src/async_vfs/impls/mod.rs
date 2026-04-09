//! Async Virtual filesystem implementations

pub mod altroot;
pub mod memory;
pub mod overlay;

#[cfg(all(feature = "tokio-physical", feature = "smol-physical"))]
compile_error!(
    "Features `tokio-physical` and `smol-physical` are mutually exclusive. Please enable only one."
);

#[cfg(feature = "tokio-physical")]
#[path = "physical_tokio.rs"]
pub mod physical;

#[cfg(all(feature = "smol-physical", not(feature = "tokio-physical")))]
#[path = "physical_smol.rs"]
pub mod physical;
