#!/usr/bin/env python3
"""Export a clean source snapshot without local Git history or private QA assets."""
from pathlib import Path
import subprocess, shutil, re, json, argparse
root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--output', type=Path, default=root / 'target/public-beta/source')
args = parser.parse_args()
out = args.output.resolve()
if out.exists():
    raise SystemExit('Export already exists; choose a fresh target before exporting again.')
files = subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=root).decode().split('\0')
roots = {'Cargo.toml','Cargo.lock','package.json','package-lock.json','index.html','vite.config.ts','vitest.config.ts','tsconfig.json','tsconfig.node.json','.gitignore','LICENSE','THIRD_PARTY.md','README.md','README.zh-CN.md'}
scripts = {'scripts/env.sh','scripts/build_sidecars.sh','scripts/prepare-agent-tools.mjs','scripts/prepare-public-beta.py','scripts/build-update-release.sh','scripts/prepare-update-manifest.py'}
def allowed(name):
    return name in roots or name in scripts or name.startswith(('src/','crates/','skills/','tests/','docs/public/','.github/','src-tauri/src/','src-tauri/tests/','src-tauri/icons/','src-tauri/capabilities/')) or name in {'src-tauri/Cargo.toml','src-tauri/build.rs','src-tauri/tauri.conf.json','src-tauri/Info.plist','assets/branding/haohaoxie-u.png','docs/UPDATES.md','docs/HELP.zh.md','docs/HELP.en.md'}
selected=sorted(set(n for n in files if n and allowed(n)))
patterns=[rb'sk-[A-Za-z0-9_-]{20,}',rb'gh[pousr]_[A-Za-z0-9]{20,}',rb'github_pat_[A-Za-z0-9_]{20,}',rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',rb'/Users/huangzongjing' + rb'/']
violations=[]
for name in selected:
    data=(root/name).read_bytes()
    if any(re.search(p,data) for p in patterns): violations.append(name)
if violations: raise SystemExit('Sensitive-pattern scan failed in files: '+', '.join(violations))
for name in selected:
    dest=out/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(root/name,dest)
(out.parent/'source-manifest.json').write_text(json.dumps({'files':selected,'count':len(selected),'scan':'No configured secret or personal-path patterns found; not a comprehensive security audit.'},indent=2)+'\n')
print(f'Exported {len(selected)} files to {out}')
