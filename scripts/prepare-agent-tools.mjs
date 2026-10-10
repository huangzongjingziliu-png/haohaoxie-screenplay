import { spawnSync } from 'node:child_process';
import { mkdirSync, copyFileSync, writeFileSync, chmodSync, readFileSync, readdirSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
const destination = 'src-tauri/resources/agent';
const signaturePath = `${destination}/build-inputs.sha256`;
const hash = createHash('sha256');
function visit(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name))) {
    const path = `${dir}/${entry.name}`;
    if (entry.isDirectory()) visit(path);
    else if (entry.name.endsWith('.rs') || entry.name.endsWith('.toml')) hash.update(path).update(readFileSync(path));
  }
}
visit('crates');
for (const path of ['Cargo.toml', 'Cargo.lock', 'scripts/prepare-agent-tools.mjs']) hash.update(readFileSync(path));
const signature = hash.digest('hex');
if (existsSync(signaturePath) && readFileSync(signaturePath, 'utf8').trim() === signature
    && ['screenplay', 'screenplay-mcp', 'screenplay-bin', 'screenplay-mcp-bin'].every(name => existsSync(`${destination}/${name}`))) {
  console.log('Bundled CLI/MCP match current sources; retaining verified binaries.');
  process.exit(0);
}
const args = ['build', '--locked', '--release', '-p', 'screenplay-cli', '-p', 'screenplay-mcp'];
const build = spawnSync('cargo', args, { stdio: 'inherit' });
if (build.status !== 0) process.exit(build.status || 1);
mkdirSync(destination, { recursive: true });
for (const name of ['screenplay', 'screenplay-mcp']) {
  copyFileSync(`target/release/${name}`, `${destination}/${name}-bin`);
  writeFileSync(`${destination}/${name}`, `#!/bin/sh
# Resolve links so installation in ~/.local/bin retains sidecar discovery.
script="$0"
while [ -L "$script" ]; do
  link=$(readlink "$script")
  case "$link" in /*) script="$link" ;; *) script="$(dirname "$script")/$link" ;; esac
done
agent_dir=$(CDPATH= cd -- "$(dirname -- "$script")" && pwd)
export SCREENPLAY_PDF_SIDECAR="$agent_dir/../pdf_sidecar"
export SCREENPLAY_OCR_SIDECAR="$agent_dir/../ocr_sidecar"
exec "$agent_dir/${name}-bin" "$@"
`);
  chmodSync(`${destination}/${name}`, 0o755);
  chmodSync(`${destination}/${name}-bin`, 0o755);
}

writeFileSync(signaturePath, signature + '\n');
