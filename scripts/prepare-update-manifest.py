#!/usr/bin/env python3
"""Generate the signed updater manifest; publish it after its referenced asset."""
import argparse,datetime,json,pathlib,urllib.parse,shutil
root=pathlib.Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--tag',required=True)
parser.add_argument('--notes',type=pathlib.Path)
args=parser.parse_args()
version=json.loads((root/'package.json').read_text())['version']
if args.tag.lstrip('v').split('-')[0]!=version:
    parser.error('Release tag must match the desktop version')
bundle=root/'target/release/bundle/macos'
product=json.loads((root/'src-tauri/tauri.conf.json').read_text())['productName']
package=bundle/(product+'.app.tar.gz')
if not package.is_file():parser.error('Updater package missing; run signed build first')
output_dir=root/'target/release/bundle/updater'
output_dir.mkdir(parents=True,exist_ok=True)
public_name=f'haohaoxie-screenplay_{version}_aarch64.app.tar.gz'
shutil.copy2(package,output_dir/public_name)
shutil.copy2(pathlib.Path(str(package)+'.sig'),output_dir/(public_name+'.sig'))
signature=pathlib.Path(str(package)+'.sig').read_text().strip()
if not signature:parser.error('Missing signature')
manifest={'version':version,'notes':args.notes.read_text() if args.notes else '',
 'pub_date':datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00','Z'),
 'platforms':{'darwin-aarch64':{'url':'https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/releases/download/'+urllib.parse.quote(args.tag,safe='')+'/'+urllib.parse.quote(public_name,safe=''),'signature':signature}}}
output=output_dir/'latest.json'
output.write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
print('Upload updater package, .sig and latest.json to the SAME release (manifest last).')
print(output)
