use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree};

struct StyxExtension;

impl zed::Extension for StyxExtension {
    fn new() -> Self {
        StyxExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        // Look for 'styx' in PATH or in the workspace
        let path = worktree.which("styx").ok_or_else(|| {
            "styx not found in PATH. Install styx CLI or add it to PATH.".to_string()
        })?;

        Ok(Command {
            command: path,
            args: vec!["@lsp".to_string()],
            env: vec![],
        })
    }
}

zed::register_extension!(StyxExtension);
