//! Live mode (spec §4): the pieces that join the MCP server to the running
//! window. This file holds what the window hears about (`Change`); Task 5
//! adds the endpoint the window listens on and the `--mcp` relay.

use std::path::PathBuf;
use std::sync::Arc;

use crate::undo::UndoOutcome;

/// What a tool call changed, for the window (spec §4.3).
#[derive(Debug)]
pub enum Change {
    /// The window's document moved — an edit, a save, an undo, a restore —
    /// with the projection the frontend lands through `landUndo()`. Boxed:
    /// `UndoOutcome` carries two full document trees, and clippy flags the
    /// bare enum as heavily skewed next to `Wrote`'s `Vec<PathBuf>`.
    Edited { tool: String, outcome: Box<UndoOutcome> },
    /// Settings files written on disk behind the open documents (a batch
    /// copy, a private workspace's restore): the frontend re-reads a clean
    /// slot and flags a dirty one, as it does after a batch.
    Wrote(Vec<PathBuf>),
}

/// The window's listener for changes; `None` headless.
pub type OnChange = Arc<dyn Fn(Change) + Send + Sync>;
