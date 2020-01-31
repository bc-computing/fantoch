// This module contains the definition of `VoteRange` and `Votes`.
mod votes;

// This module contains the definition of `KeyClocks` and `QuorumClocks`.
mod clocks;

// Re-exports.
pub use clocks::{KeyClocks, QuorumClocks, SequentialKeyClocks};
pub use votes::{VoteRange, Votes};
