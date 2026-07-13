//! EVE settings document handling on top of the `blue-marshal` codec:
//! fidelity-checked loading, JSON tree projection, mutations, the
//! backup/verify/atomic save chain, backups, and profile discovery.
//! No EVE *semantics* live here yet (categories arrive in M2/M3).

pub mod backups;
pub mod discover;
pub mod document;
pub mod mutate;
pub mod path;
pub mod projection;
pub mod save;

pub use document::{Document, Fidelity, LoadError};
// pub use mutate::{apply, Mutation, MutateError, NewValue}; // enabled in Task 4
pub use path::{resolve, resolve_mut, NodePath, Step};
pub use projection::{project, Node}; // enabled in Task 4
// pub use save::{save, SaveError, SaveReport}; // enabled in Task 6
