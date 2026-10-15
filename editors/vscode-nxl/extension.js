// NXL language support for VS Code.
//
// The extension runs the real compiler in two ways: as commands that print its
// output (run, check, build), and as a language server (`nxlc lsp`) that
// provides completion, hover, go-to-definition, find-references, the symbol
// outline and diagnostics.
//
// The language client is a dependency-free JSON-RPC client in
// language-client.js, so the extension can be loaded from source without an
// install step. If the compiler cannot be started, the commands still work
// and the editor features are simply absent - they are never faked.

const vscode = require('vscode');
const { spawn } = require('child_process');
const { LanguageClient, toPosition, toRange } = require('./language-client');

/** Path to the compiler, honouring the `nxl.compilerPath` setting. */
function compilerPath() {
  return vscode.workspace
    .getConfiguration('nxl')
    .get('compilerPath', 'nxlc');
}

/** Run nxlc and stream its output into a channel. */
function run(args, cwd) {
  const channel = vscode.window.createOutputChannel('NXL');
  channel.show(true);
  channel.appendLine(`> ${compilerPath()} ${args.join(' ')}   (${cwd})`);

  const child = spawn(compilerPath(), args, { cwd, shell: process.platform === 'win32' });
  child.stdout.on('data', (chunk) => channel.append(chunk.toString()));
  child.stderr.on('data', (chunk) => channel.append(chunk.toString()));
  child.on('error', (error) => {
    channel.appendLine(
      `could not start ${compilerPath()}: ${error.message}\n` +
        'Install the NEXORA toolchain, or set nxl.compilerPath to its full path.'
    );
  });
  child.on('close', (code) => {
    channel.appendLine(`\n[exit ${code}]`);
  });
}

/** The directory a project command should run in. */
function projectRoot() {
  const folder = vscode.workspace.workspaceFolders?.[0];
  return folder ? folder.uri.fsPath : process.cwd();
}

/** The .nxl file in the active editor, or undefined. */
function activeNxlFile() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== 'nxl') {
    return undefined;
  }
  return editor.document;
}

function register(context, id, title, handler) {
  context.subscriptions.push(
    vscode.commands.registerCommand(id, async () => {
      try {
        await handler();
      } catch (error) {
        vscode.window.showErrorMessage(`NXL: ${error.message}`);
      }
    })
  );
}

function activate(context) {
  register(context, 'nxl.runFile', 'Run file', async () => {
    const file = activeNxlFile();
    if (!file) {
      vscode.window.showInformationMessage('NXL: open an .nxl file first.');
      return;
    }
    run(['run', file.uri.fsPath], projectRoot());
  });

  register(context, 'nxl.checkFile', 'Check file', async () => {
    const file = activeNxlFile();
    if (!file) {
      vscode.window.showInformationMessage('NXL: open an .nxl file first.');
      return;
    }
    run(['check', file.uri.fsPath], projectRoot());
  });

  register(context, 'nxl.buildProject', 'Build project', async () => {
    run(['build', '--release'], projectRoot());
  });

  startLanguageServer(context);
}

/**
 * Connect `nxlc lsp` and wire its answers to editor features.
 *
 * Everything here is best-effort: a compiler that is missing, crashes or does
 * not speak the protocol leaves the editor working with syntax highlighting
 * and the commands above, which is a degraded editor rather than a broken one.
 */
function startLanguageServer(context) {
  const channel = vscode.window.createOutputChannel('NXL Language Server');
  const log = (message) => channel.appendLine(message);
  const diagnostics = vscode.languages.createDiagnosticCollection('nxl');
  context.subscriptions.push(diagnostics, channel);

  const client = new LanguageClient(
    (uri, items) => {
      diagnostics.set(
        vscode.Uri.parse(uri),
        items.map((item) => {
          const severity = item.severity === 2 ? vscode.DiagnosticSeverity.Warning : vscode.DiagnosticSeverity.Error;
          const diagnostic = new vscode.Diagnostic(
            toRange(item.range),
            item.message || '',
            severity
          );
          diagnostic.source = item.source || 'nxlc';
          return diagnostic;
        })
      );
    },
    log
  );
  context.subscriptions.push({ dispose: () => client.stop() });

  // Keep the server's view of each open document in step with the editor.
  const opened = new Map();

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((document) => {
      if (document.languageId !== 'nxl' || !client.running) {
        return;
      }
      opened.set(document.uri.toString(), document);
      client.didOpen(document);
    }),
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.document.languageId !== 'nxl' || !client.running) {
        return;
      }
      client.didChange(event.document);
    }),
    vscode.workspace.onDidCloseTextDocument((document) => {
      if (document.languageId !== 'nxl') {
        return;
      }
      opened.delete(document.uri.toString());
      diagnostics.delete(document.uri);
      if (client.running) {
        client.didClose(document);
      }
    }),
    vscode.workspace.onDidSaveTextDocument((document) => {
      if (document.languageId === 'nxl' && client.running) {
        client.didSave(document);
      }
    })
  );

  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(
      { language: 'nxl' },
      {
        async provideCompletionItems(document, position) {
          if (!client.running) {
            return undefined;
          }
          const result = await client.request('textDocument/completion', {
            textDocument: { uri: document.uri.toString() },
            position: toPosition(position),
          });
          if (!result || !Array.isArray(result.items)) {
            return undefined;
          }
          return result.items.map((item) => new vscode.CompletionItem(item.label));
        }
      },
      '.',
      ' '
    ),
    vscode.languages.registerHoverProvider({ language: 'nxl' }, {
      async provideHover(document, position) {
        if (!client.running) {
          return undefined;
        }
        const result = await client.request('textDocument/hover', {
          textDocument: { uri: document.uri.toString() },
          position: toPosition(position),
        });
        if (!result || !result.contents) {
          return undefined;
        }
        const value =
          typeof result.contents === 'string'
            ? result.contents
            : result.contents.value;
        if (!value) {
          return undefined;
        }
        return new vscode.Hover(new vscode.MarkdownString(value));
      }
    }),
    vscode.languages.registerDefinitionProvider({ language: 'nxl' }, {
      async provideDefinition(document, position) {
        if (!client.running) {
          return undefined;
        }
        const result = await client.request('textDocument/definition', {
          textDocument: { uri: document.uri.toString() },
          position: toPosition(position),
        });
        if (!Array.isArray(result) || result.length === 0) {
          return undefined;
        }
        return result.map((location) => new vscode.Location(vscode.Uri.parse(location.uri), toRange(location.range)));
      }
    }),
    vscode.languages.registerReferenceProvider({ language: 'nxl' }, {
      async provideReferences(document, position) {
        if (!client.running) {
          return undefined;
        }
        const result = await client.request('textDocument/references', {
          textDocument: { uri: document.uri.toString() },
          position: toPosition(position),
        });
        if (!Array.isArray(result)) {
          return undefined;
        }
        return result.map((location) => new vscode.Location(vscode.Uri.parse(location.uri), toRange(location.range)));
      }
    }),
    vscode.languages.registerDocumentSymbolProvider({ language: 'nxl' }, {
      async provideDocumentSymbols(document) {
        if (!client.running) {
          return undefined;
        }
        const result = await client.request('textDocument/documentSymbol', {
          textDocument: { uri: document.uri.toString() },
        });
        if (!Array.isArray(result)) {
          return undefined;
        }
        return result.map((symbol) => {
          const range = toRange(symbol.range);
          return new vscode.SymbolInformation(
            symbol.name,
            symbolKind(symbol.kind),
            new vscode.Range(range.start, range.start),
            vscode.Uri.parse(document.uri.toString())
          );
        });
      }
    })
  );

  client.start(compilerPath()).then((started) => {
    if (!started) {
      log(
        `no language server: ${compilerPath()} could not be started.\n` +
          'Syntax highlighting and the run/check/build commands still work.'
      );
      return;
    }
    log('connected to nxlc lsp: completion, hover, definition, references and diagnostics are live');
    // Documents already open when the server started were never sent.
    for (const document of vscode.workspace.textDocuments) {
      if (document.languageId === 'nxl') {
        opened.set(document.uri.toString(), document);
        client.didOpen(document);
      }
    }
  });
}

/** Map an LSP SymbolKind number onto the editor's. */
function symbolKind(kind) {
  const vscode = require('vscode');
  const table = {
    2: vscode.SymbolKind.Module,
    5: vscode.SymbolKind.Class,
    6: vscode.SymbolKind.Method,
    10: vscode.SymbolKind.Enum,
    11: vscode.SymbolKind.Interface,
    12: vscode.SymbolKind.Function,
    13: vscode.SymbolKind.Variable,
    23: vscode.SymbolKind.Struct,
  };
  return table[kind] || vscode.SymbolKind.Variable;
}

function deactivate() {}

module.exports = { activate, deactivate };