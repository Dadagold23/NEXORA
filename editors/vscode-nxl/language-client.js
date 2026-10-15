// A minimal Language Server Protocol client for NXL.
//
//   const client = new LanguageClient(onDiagnostics, onLog);
//   client.start(compilerPath);
//   client.didOpen(document);
//   client.didChange(document);
//   client.requests.completion(position);
//
// It speaks the real protocol over the server's stdio - Content-Length framed
// JSON-RPC - rather than wrapping `vscode-languageclient`, so the extension
// keeps having no dependencies and can be loaded from source without an
// install step. It implements only what `nxlc lsp` answers.

const { spawn } = require('child_process');

/** Milliseconds before a request is abandoned rather than waited on forever. */
const REQUEST_TIMEOUT_MS = 10000;

class LanguageClient {
  /**
   * @param {(uri: string, diagnostics: object[]) => void} onDiagnostics
   * @param {(message: string) => void} onLog
   */
  constructor(onDiagnostics, onLog) {
    this.child = null;
    this.buffer = Buffer.alloc(0);
    this.pending = new Map();
    this.nextId = 1;
    this.onDiagnostics = onDiagnostics;
    this.onLog = onLog;
    this.requests = {};
    this.stopped = false;
  }

  /** True once a server process is running. */
  get running() {
    return this.child !== null;
  }

  /** Start `nxlc lsp`. Resolves false when the compiler cannot be started. */
  start(compilerPath) {
    return new Promise((resolve) => {
      let child;
      try {
        child = spawn(compilerPath, ['lsp'], {
          shell: process.platform === 'win32',
        });
      } catch (error) {
        this.onLog(`could not start ${compilerPath}: ${error.message}`);
        resolve(false);
        return;
      }

      child.on('error', (error) => {
        this.onLog(
          `could not start ${compilerPath}: ${error.message}\n` +
            'Install the NEXORA toolchain, or set nxl.compilerPath to its full path.'
        );
        this.child = null;
        resolve(false);
      });

      child.stdout.on('data', (chunk) => this.receive(chunk));
      child.stderr.on('data', (chunk) => this.onLog(chunk.toString()));

      child.on('close', (code) => {
        this.onLog(`language server exited (${code})`);
        this.child = null;
        // Nothing can answer now; fail the waiters rather than leaking them.
        for (const [, waiter] of this.pending) {
          clearTimeout(waiter.timer);
          waiter.reject(new Error('the NXL language server stopped'));
        }
        this.pending.clear();
        if (!this.stopped) {
          // An unexpected exit is worth surfacing once, not on every keystroke.
          this.onLog('completion, hover and diagnostics are unavailable until it restarts');
        }
      });

      this.child = child;
      this.initialize()
        .then(() => resolve(true))
        .catch((error) => {
          this.onLog(`language server did not initialise: ${error.message}`);
          resolve(false);
        });
    });
  }

  /** Shut the server down, politely. */
  async stop() {
    if (!this.child) {
      return;
    }
    this.stopped = true;
    try {
      await this.request('shutdown', {});
    } catch (error) {
      // Already gone; the exit below is what matters.
    }
    this.notify('exit', {});
    this.child.kill();
    this.child = null;
  }

  // ── protocol plumbing ──────────────────────────────────────────────────

  receive(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    for (;;) {
      const marker = this.buffer.indexOf('\r\n\r\n');
      if (marker < 0) {
        return;
      }
      const header = this.buffer.subarray(0, marker).toString('ascii');
      const match = /Content-Length:\s*(\d+)/i.exec(header);
      if (!match) {
        // Unusable frame; drop it rather than spin on it forever.
        this.buffer = this.buffer.subarray(marker + 4);
        continue;
      }
      const length = Number(match[1]);
      const start = marker + 4;
      if (this.buffer.length < start + length) {
        return;
      }
      const body = this.buffer.subarray(start, start + length).toString('utf8');
      this.buffer = this.buffer.subarray(start + length);
      try {
        this.dispatch(JSON.parse(body));
      } catch (error) {
        this.onLog(`malformed message from the language server: ${error.message}`);
      }
    }
  }

  dispatch(message) {
    if (message.id !== undefined && message.id !== null && this.pending.has(message.id)) {
      const waiter = this.pending.get(message.id);
      this.pending.delete(message.id);
      clearTimeout(waiter.timer);
      if (message.error) {
        waiter.reject(new Error(message.error.message));
      } else {
        waiter.resolve(message.result);
      }
      return;
    }
    if (message.method === 'textDocument/publishDiagnostics') {
      const params = message.params || {};
      this.onDiagnostics(params.uri, params.diagnostics || []);
    }
  }

  write(payload) {
    if (!this.child) {
      return;
    }
    const body = Buffer.from(JSON.stringify(payload), 'utf8');
    this.child.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    this.child.stdin.write(body);
  }

  /** Send a request and wait for its reply. */
  request(method, params) {
    return new Promise((resolve, reject) => {
      if (!this.child) {
        reject(new Error('the NXL language server is not running'));
        return;
      }
      const id = this.nextId++;
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`${method} timed out`));
      }, REQUEST_TIMEOUT_MS);
      this.pending.set(id, { resolve, reject, timer });
      this.write({ jsonrpc: '2.0', id, method, params });
    });
  }

  /** Send a notification; nothing is expected back. */
  notify(method, params) {
    this.write({ jsonrpc: '2.0', method, params });
  }

  initialize() {
    return this.request('initialize', {
      processId: process.pid,
      rootUri: null,
      capabilities: {
        textDocument: {
          completion: { completionItem: { snippetSupport: false } },
          hover: { contentFormat: ['markdown', 'plaintext'] },
          publishDiagnostics: { relatedInformation: false },
        },
      },
      clientInfo: { name: 'nxl-vscode' },
    }).then(() => {
      this.notify('initialized', {});
    });
  }

  // ── document synchronisation ───────────────────────────────────────────

  didOpen(document) {
    this.notify('textDocument/didOpen', {
      textDocument: {
        uri: document.uri.toString(),
        languageId: 'nxl',
        version: document.version,
        text: document.getText(),
      },
    });
  }

  didChange(document) {
    this.notify('textDocument/didChange', {
      // The server is registered for full synchronisation, so the whole
      // document goes up on every change.
      textDocument: { uri: document.uri.toString(), version: document.version },
      contentChanges: [{ text: document.getText() }],
    });
  }

  didClose(document) {
    this.notify('textDocument/didClose', {
      textDocument: { uri: document.uri.toString() },
    });
  }

  didSave(document) {
    this.notify('textDocument/didSave', {
      textDocument: { uri: document.uri.toString() },
    });
  }
}

/** Convert a VS Code position into the LSP shape. */
function toPosition(position) {
  return { line: position.line, character: position.character };
}

/** Convert an LSP range into the VS Code shape. */
function toRange(range) {
  const vscode = require('vscode');
  const start = range && range.start ? range.start : { line: 0, character: 0 };
  const end = range && range.end ? range.end : start;
  return new vscode.Range(start.line, start.character, end.line, end.character);
}

module.exports = { LanguageClient, toPosition, toRange };