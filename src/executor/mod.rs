// This module contains the implementation of a dependency graph.
mod graph;

// This module contains the implementation of a votes table.
mod table;

// Re-exports.
pub use graph::{GraphExecutionInfo, GraphExecutor};
pub use table::{TableExecutionInfo, TableExecutor};

use crate::command::{Command, CommandResult};
use crate::config::Config;

pub trait Executor {
    type ExecutionInfo;

    fn new(config: &Config) -> Self;

    fn register(&mut self, cmd: &Command);

    fn handle(&mut self, infos: Vec<Self::ExecutionInfo>) -> Vec<CommandResult>;

    fn show_metrics(&mut self) {
        // by default, nothing to show
    }
}
