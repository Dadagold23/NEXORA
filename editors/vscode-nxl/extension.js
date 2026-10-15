// NXL language support for VS Code: run, check and build with nxlc.
//
// Deliberately small and honest: it shells out to the real compiler and shows
// its output. There is no language server here yet, so no completion, hover or
// diagnostics beyond what `nxlc check` reports.

const vscode = require('vscode');
const { spawn } = require('child_process');

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
}

function deactivate() {}

module.exports = { activate, deactivate };