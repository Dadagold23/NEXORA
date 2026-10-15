// Checks the extension manifest without needing the VS Code toolchain.
//
//   node scripts/verify-extension.js
//
// Run with `npm run verify` inside this directory. Exits non-zero when
// something an editor needs is missing.

const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..');
const problems = [];

function check(condition, message) {
  if (!condition) {
    problems.push(message);
  }
}

const manifestPath = path.join(root, 'package.json');
check(fs.existsSync(manifestPath), 'package.json is missing');
if (!fs.existsSync(manifestPath)) {
  report();
}

const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));

check(typeof manifest.name === 'string' && manifest.name.length > 0, 'name is missing');
check(/^\d+\.\d+\.\d+$/.test(manifest.version || ''), 'version must be semver');
check(
  typeof manifest.engines?.vscode === 'string' || Array.isArray(manifest.engines?.vscode),
  'engines.vscode must declare a VS Code range'
);

// The marketplace rejects icons that are missing, tiny or not square.
const icon = manifest.icon ? path.join(root, manifest.icon) : null;
if (icon) {
  check(fs.existsSync(icon), `icon ${manifest.icon} is missing`);
} else {
  check(false, 'no top-level icon (the marketplace requires one)');
}

const language = (manifest.contributes?.languages || [])[0];
check(!!language, 'contributes.languages is empty');
if (language) {
  check(language.id === 'nxl', 'the language id should be "nxl"');
  check(
    (language.extensions || []).includes('.nxl'),
    'the language must claim the .nxl extension'
  );
  check(
    typeof language.icon === 'string',
    'the language needs an icon for the file explorer'
  );
  if (language.icon) {
    const fileIcon = path.join(root, language.icon);
    check(fs.existsSync(fileIcon), `language icon ${language.icon} is missing`);
    // VS Code renders language icons as SVG or PNG; .ico is not supported.
    check(
      /\.(svg|png)$/i.test(language.icon),
      `language icon must be .svg or .png, not ${language.icon}`
    );
  }
}

const grammar = (manifest.contributes?.grammars || [])[0];
check(!!grammar, 'contributes.grammars is empty');
if (grammar) {
  check(fs.existsSync(path.join(root, grammar.path)), `${grammar.path} is missing`);
  if (fs.existsSync(path.join(root, grammar.path))) {
    JSON.parse(fs.readFileSync(path.join(root, grammar.path), 'utf8'));
  }
}

if (language?.configuration) {
  check(
    fs.existsSync(path.join(root, language.configuration)),
    `${language.configuration} is missing`
  );
}

for (const command of manifest.contributes?.commands || []) {
  check(typeof command.command === 'string', 'a command has no id');
  check(typeof command.title === 'string', `command ${command.command} has no title`);
}

if (fs.existsSync(path.join(root, 'extension.js'))) {
  const source = fs.readFileSync(path.join(root, 'extension.js'), 'utf8');
  // eslint-disable-next-line no-new-func
  new Function('require', 'module', 'exports', source);
}

function report() {
  if (problems.length > 0) {
    console.error('extension verification failed:');
    for (const problem of problems) {
      console.error(`  - ${problem}`);
    }
    process.exit(1);
  }
  console.log('extension manifest verified');
  process.exit(0);
}

report();