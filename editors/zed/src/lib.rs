use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree};

struct VixExtension;

impl zed::Extension for VixExtension {
    fn new() -> Self {
        VixExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let settings = zed::settings::LspSettings::for_worktree("vix-lsp", worktree)?;
        let binary = settings.binary;
        let command = if let Some(path) = binary.as_ref().and_then(|binary| binary.path.clone()) {
            path
        } else {
            worktree.which("vix-lsp").ok_or_else(|| {
                "vix-lsp not found in PATH. Build vix-lsp or set lsp.vix-lsp.binary.path."
                    .to_string()
            })?
        };

        let mut env: Vec<(String, String)> = binary
            .as_ref()
            .and_then(|binary| binary.env.clone())
            .unwrap_or_default()
            .into_iter()
            .collect();
        if !env.iter().any(|(key, _)| key == "VIX_LSP_LOG_DIR") {
            env.push(("VIX_LSP_LOG_DIR".to_string(), "/tmp/vix-lsp".to_string()));
        }
        if !env.iter().any(|(key, _)| key == "VIX_LSP_LOG_LEVEL") {
            env.push(("VIX_LSP_LOG_LEVEL".to_string(), "info".to_string()));
        }
        if !env.iter().any(|(key, _)| key == "VIX_LSP_LOG_RETENTION") {
            env.push(("VIX_LSP_LOG_RETENTION".to_string(), "7".to_string()));
        }

        Ok(Command {
            command,
            args: binary
                .and_then(|binary| binary.arguments)
                .unwrap_or_default(),
            env,
        })
    }
}

zed::register_extension!(VixExtension);
