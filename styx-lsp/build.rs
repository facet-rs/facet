use facet_styx::GenerateSchema;

// Include the config module to get access to StyxUserConfig
mod config_types {
    use facet::Facet;

    /// User configuration for the Styx LSP.
    #[derive(Debug, Clone, Default, Facet)]
    pub struct StyxUserConfig {
        /// LSP extensions that are allowed to run.
        ///
        /// When a schema specifies an LSP extension (via `meta.lsp.launch`),
        /// the extension command must be in this list to be spawned.
        /// Extensions can be added via the "Allow LSP extension" code action.
        #[facet(default)]
        pub allowed_extensions: Vec<String>,
    }
}

fn main() {
    GenerateSchema::<config_types::StyxUserConfig>::new()
        .crate_name("styx-lsp-config")
        .version("1")
        .write("config.schema.styx");
}
