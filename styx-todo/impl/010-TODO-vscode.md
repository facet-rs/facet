# Phase 010: vscode-styx (VS Code Extension)

VS Code extension for Styx, providing syntax highlighting, LSP integration, and schema support.

## Deliverables

- `editors/vscode-styx/` - Extension package
- `editors/vscode-styx/package.json` - Extension manifest
- `editors/vscode-styx/syntaxes/styx.tmLanguage.json` - TextMate grammar
- `editors/vscode-styx/language-configuration.json` - Language config
- `editors/vscode-styx/src/extension.ts` - Extension entry point

## Features

### Syntax Highlighting (TextMate Grammar)

Basic highlighting without LSP:
- Comments (`//`, `///`)
- Strings (quoted, raw, heredoc)
- Structural tokens (`{`, `}`, `(`, `)`, `=`, `,`)
- Tags (`@name`)
- Unit (`@`)

### LSP Integration

When styx-ls is available:
- Semantic highlighting (schema-aware)
- Diagnostics (syntax + schema errors)
- Completions (keys, values, tags from schema)
- Hover (type info, documentation)
- Formatting
- Go to definition (schema types)

### Schema Association

```json
// .vscode/settings.json
{
  "styx.schema.associations": {
    "config/*.styx": "./schemas/config.schema.styx",
    "k8s/**/*.styx": "./schemas/kubernetes.schema.styx"
  }
}
```

### Snippets

```json
{
  "Object": {
    "prefix": "obj",
    "body": ["${1:key} {", "\t$0", "}"]
  },
  "Sequence": {
    "prefix": "seq",
    "body": ["${1:key} (", "\t$0", ")"]
  },
  "Heredoc": {
    "prefix": "heredoc",
    "body": ["${1:key} <<${2:EOF}", "$0", "${2:EOF}"]
  }
}
```

## Package Structure

```
editors/vscode-styx/
├── package.json
├── tsconfig.json
├── src/
│   └── extension.ts
├── syntaxes/
│   └── styx.tmLanguage.json
├── language-configuration.json
├── snippets/
│   └── styx.json
├── schemas/
│   └── settings.schema.json    # VS Code settings schema
└── README.md
```

## package.json

```json
{
  "name": "vscode-styx",
  "displayName": "Styx",
  "description": "Styx configuration language support",
  "version": "0.1.0",
  "publisher": "bearcove",
  "repository": "https://github.com/bearcove/styx",
  "engines": {
    "vscode": "^1.85.0"
  },
  "categories": ["Programming Languages", "Linters", "Formatters"],
  "activationEvents": [
    "onLanguage:styx"
  ],
  "main": "./out/extension.js",
  "contributes": {
    "languages": [{
      "id": "styx",
      "aliases": ["Styx", "styx"],
      "extensions": [".styx"],
      "configuration": "./language-configuration.json",
      "icon": {
        "light": "./icons/styx-light.svg",
        "dark": "./icons/styx-dark.svg"
      }
    }],
    "grammars": [{
      "language": "styx",
      "scopeName": "source.styx",
      "path": "./syntaxes/styx.tmLanguage.json"
    }],
    "snippets": [{
      "language": "styx",
      "path": "./snippets/styx.json"
    }],
    "configuration": {
      "title": "Styx",
      "properties": {
        "styx.server.path": {
          "type": "string",
          "default": "styx-ls",
          "description": "Path to styx-ls executable"
        },
        "styx.schema.associations": {
          "type": "object",
          "default": {},
          "description": "Associate glob patterns with schema files"
        },
        "styx.format.onSave": {
          "type": "boolean",
          "default": false,
          "description": "Format document on save"
        }
      }
    },
    "commands": [{
      "command": "styx.restartServer",
      "title": "Styx: Restart Language Server"
    }]
  },
  "scripts": {
    "vscode:prepublish": "npm run compile",
    "compile": "tsc -p ./",
    "watch": "tsc -watch -p ./"
  },
  "dependencies": {
    "vscode-languageclient": "^9.0.0"
  },
  "devDependencies": {
    "@types/vscode": "^1.85.0",
    "typescript": "^5.0.0"
  }
}
```

## TextMate Grammar

```json
{
  "name": "Styx",
  "scopeName": "source.styx",
  "patterns": [
    { "include": "#comments" },
    { "include": "#strings" },
    { "include": "#tags" },
    { "include": "#punctuation" }
  ],
  "repository": {
    "comments": {
      "patterns": [
        {
          "name": "comment.line.documentation.styx",
          "match": "///.*$"
        },
        {
          "name": "comment.line.styx",
          "match": "//.*$"
        }
      ]
    },
    "strings": {
      "patterns": [
        {
          "name": "string.quoted.double.styx",
          "begin": "\"",
          "end": "\"",
          "patterns": [
            {
              "name": "constant.character.escape.styx",
              "match": "\\\\."
            }
          ]
        },
        {
          "name": "string.quoted.raw.styx",
          "begin": "r(#*)\"",
          "end": "\"\\1"
        },
        {
          "name": "string.unquoted.heredoc.styx",
          "begin": "<<([A-Z_][A-Z0-9_]*)",
          "end": "^\\1$",
          "beginCaptures": {
            "0": { "name": "punctuation.definition.string.begin.styx" }
          },
          "endCaptures": {
            "0": { "name": "punctuation.definition.string.end.styx" }
          }
        }
      ]
    },
    "tags": {
      "patterns": [
        {
          "name": "entity.name.tag.styx",
          "match": "@[a-zA-Z_][a-zA-Z0-9_]*"
        },
        {
          "name": "constant.language.unit.styx",
          "match": "@(?![a-zA-Z_])"
        }
      ]
    },
    "punctuation": {
      "patterns": [
        {
          "name": "punctuation.definition.block.styx",
          "match": "[{}()]"
        },
        {
          "name": "punctuation.separator.styx",
          "match": "[,=]"
        }
      ]
    }
  }
}
```

## Language Configuration

```json
{
  "comments": {
    "lineComment": "//"
  },
  "brackets": [
    ["{", "}"],
    ["(", ")"]
  ],
  "autoClosingPairs": [
    { "open": "{", "close": "}" },
    { "open": "(", "close": ")" },
    { "open": "\"", "close": "\"", "notIn": ["string"] }
  ],
  "surroundingPairs": [
    ["{", "}"],
    ["(", ")"],
    ["\"", "\""]
  ],
  "folding": {
    "markers": {
      "start": "^\\s*\\{",
      "end": "^\\s*\\}"
    }
  },
  "indentationRules": {
    "increaseIndentPattern": "^.*\\{[^}]*$",
    "decreaseIndentPattern": "^\\s*\\}"
  }
}
```

## Extension Entry Point

```typescript
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration('styx');
  const serverPath = config.get<string>('server.path', 'styx-ls');

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: [],
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'styx' }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.styx'),
    },
  };

  client = new LanguageClient(
    'styx',
    'Styx Language Server',
    serverOptions,
    clientOptions
  );

  client.start();

  // Register restart command
  context.subscriptions.push(
    vscode.commands.registerCommand('styx.restartServer', async () => {
      if (client) {
        await client.restart();
        vscode.window.showInformationMessage('Styx language server restarted');
      }
    })
  );
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
```

## Installation

### From Marketplace

```bash
code --install-extension bearcove.vscode-styx
```

### From VSIX

```bash
cd editors/vscode-styx
npm install
npm run compile
npx vsce package
code --install-extension vscode-styx-0.1.0.vsix
```

### Development

```bash
cd editors/vscode-styx
npm install
npm run watch
# Press F5 in VS Code to launch Extension Development Host
```

## Testing

- Unit tests for extension logic
- Integration tests with VS Code test runner
- Grammar tests with vscode-tmgrammar-test
