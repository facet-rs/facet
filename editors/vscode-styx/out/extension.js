"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    const config = vscode_1.workspace.getConfiguration('styx');
    const serverPath = config.get('server.path', 'styx');
    // Server runs via: styx @lsp
    const serverOptions = {
        command: serverPath,
        args: ['@lsp'],
    };
    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'styx' }],
        synchronize: {
            fileEvents: vscode_1.workspace.createFileSystemWatcher('**/*.styx'),
        },
    };
    client = new node_1.LanguageClient('styx', 'Styx Language Server', serverOptions, clientOptions);
    client.start();
}
function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
