//! Static command expansion contract; handlers do not own journal or process state.
use crate::config::{ProjectConfig, RecipeConfig, Result};
use winbridge_protocol::{CommandHandlerInfo, CommandSource};

pub struct CommandContext<'a> {
    pub project: &'a ProjectConfig,
    pub recipes: &'a [RecipeConfig],
    pub timeout_ms: u64,
}
/// Cwd is still relative. The shared runner applies filesystem scope checks.
pub struct CommandDefinition {
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub timeout_ms: u64,
}
pub trait CommandHandler: Sync {
    fn metadata(&self) -> CommandHandlerInfo;
    fn supports(&self, source: &CommandSource) -> bool;
    fn resolve(
        &self,
        context: &CommandContext<'_>,
        source: &CommandSource,
    ) -> Result<CommandDefinition>;
}
