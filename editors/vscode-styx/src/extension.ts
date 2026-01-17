import * as path from 'path';
import { workspace, ExtensionContext } from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
  const config = workspace.getConfiguration('styx');
  const serverPath = config.get<string>('server.path', 'styx');

  // Server runs via: styx @lsp
  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ['@lsp'],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'styx' }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher('**/*.styx'),
    },
  };

  client = new LanguageClient(
    'styx',
    'Styx Language Server',
    serverOptions,
    clientOptions
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
