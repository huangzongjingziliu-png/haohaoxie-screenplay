#!/usr/bin/env python3
"""Generate the signed updater manifest; publish it after its referenced asset."""
import argparse,datetime,json,pathlib,urllib.parse
root=pathlib.Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--tag',required=True)
parser.add_argument('--notes',type=pathlib.Path)
args=parser.parse_args()
version=json.loads((root/'package.json').read_text())['version']
if args.tag.lstrip('v').split('-')[0]!=version:
    parser.error('Release tag must match the desktop version')
bundle=root/'target/release/bundle/macos'
packages=list(bundle.glob('*.app.tar.gz'))
if len(packages)!=1:parser.error('Expected one macOS updater package; run signed build first')
package=packages[0]
signature=pathlib.Path(str(package)+'.sig').read_text().strip()
if not signature:parser.error('Missing signature')
manifest={'version':version,'notes':args.notes.read_text() if args.notes else '',
 'pub_date':datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00','Z'),
 'platforms':{'darwin-aarch64':{'url':'https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/releases/download/'+urllib.parse.quote(args.tag,safe='')+'/'+urllib.parse.quote(package.name,safe=''),'signature':signature}}}
output=bundle/'latest.json'
output.write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
print('Upload updater package, .sig and latest.json to the SAME release (manifest last).')
print(output)
