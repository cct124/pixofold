// 桌面构建钩子：受控构建→固定资源目录；运行时绝不读取此清单建立信任。
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import {
  root,
  target,
  engine,
  source,
  buildIdentity,
  buildTools,
  verifyTools,
  digest,
} from './jpeg-lab/build.mjs';

const [command, ...args] = process.argv.slice(2);
if (command !== 'prepare' || (args.length && !(args.length === 2 && args[0] === '--cmake')))
  throw new Error('参数：prepare [--cmake PATH]');
await buildTools(args[1]);
const tools = verifyTools();
const directory = path.join(root, 'src-tauri', 'resources', 'jpeg', 'runtime');
mkdirSync(directory, { recursive: true });
const binary = 'pixofold-jpeg-helper' + (process.platform === 'win32' ? '.exe' : '');
const stale = path.join(
  directory,
  process.platform === 'win32' ? 'pixofold-jpeg-helper' : 'pixofold-jpeg-helper.exe',
);
if (existsSync(stale)) unlinkSync(stale);
const files = [
  [tools.helper, binary],
  [path.join(source, 'LICENSE.md'), 'MozJPEG-LICENSE.md'],
  [path.join(source, 'README.ijg'), 'README.ijg'],
  [path.join(root, 'LICENSE'), 'PixoFold-LICENSE'],
];
for (const [from, name] of files) copyFileSync(from, path.join(directory, name));
const notice =
  'This software is based in part on the work of the Independent JPEG Group.\n' +
  'MozJPEG ' +
  engine.tag +
  ', commit ' +
  engine.commit +
  '\nSource: ' +
  engine.archive +
  '\n' +
  'Upstream source is unmodified. PixoFold helper source/build recipe: native/jpeg/helper.c and tools/jpeg-lab/ in the matching PixoFold source release (GPL-3.0-or-later).\n';
writeFileSync(path.join(directory, 'NOTICE.txt'), notice);
const hashes = Object.fromEntries(
  [...files.map(([, name]) => name), 'NOTICE.txt'].map((name) => [
    name,
    digest(readFileSync(path.join(directory, name))),
  ]),
);
const manifest = { schema: 1, binary, ...buildIdentity, files: hashes };
writeFileSync(path.join(directory, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
console.log('JPEG桌面资源已准备：' + target + ' / ' + hashes[binary]);
