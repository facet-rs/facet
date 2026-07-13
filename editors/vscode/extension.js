const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

function activate(context) {
  const config = vscode.workspace.getConfiguration("vix");
  const command = config.get("server.path", "vix-lsp");
  const logDir = config.get("server.logDir", "/tmp/vix-lsp");
  const logLevel = config.get("server.logLevel", "info");
  const logRetention = String(config.get("server.logRetention", 7));
  const serverOptions = {
    command,
    options: {
      env: {
        ...process.env,
        VIX_LSP_LOG_DIR: logDir,
        VIX_LSP_LOG_LEVEL: logLevel,
        VIX_LSP_LOG_RETENTION: logRetention,
      },
    },
  };
  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "vix" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.vix"),
    },
  };
  client = new LanguageClient("vix-lsp", "Vix LSP", serverOptions, clientOptions);
  client.start();
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
