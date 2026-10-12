// 共用固定配方准备桌面资源；清单只供构建期核验，不能作为运行时授权。
import assert from 'node:assert/strict';
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
  archive,
  source,
  helper,
  definitions,
  buildTools,
  verifyTools,
  digest,
} from './gif-lab/build.mjs';

const [command, ...args] = process.argv.slice(2);
if (command !== 'prepare' || (args.length && !(args.length === 2 && args[0] === '--cmake')))
  throw new Error('参数：prepare [--cmake PATH]');
await buildTools(args[1]);
const identity = verifyTools();
assert.equal(digest(readFileSync(archive)), engine.sha256, '随包源码归档身份不符');
const directory = path.join(root, 'src-tauri/resources/gif/runtime');
mkdirSync(directory, { recursive: true });
const binary = 'pixofold-gif-helper' + (process.platform === 'win32' ? '.exe' : '');
const stale = path.join(
  directory,
  process.platform === 'win32' ? 'pixofold-gif-helper' : 'pixofold-gif-helper.exe',
);
if (existsSync(stale)) unlinkSync(stale);
const sources = Object.fromEntries(
  [...definitions, 'tools/gif-bundle.mjs'].map((file) => [
    file,
    digest(readFileSync(path.join(root, file))),
  ]),
);
const files = [
  [helper, binary],
  [archive, 'gifsicle-source.tar.gz'],
  [path.join(source, 'COPYING'), 'Gifsicle-COPYING'],
  [path.join(source, 'README.md'), 'Gifsicle-README.md'],
  [path.join(root, 'LICENSE'), 'PixoFold-LICENSE'],
  [path.join(root, 'native/gif/README.md'), 'source/native/gif/README.md'],
  ...Object.keys(sources).map((file) => [path.join(root, file), 'source/' + file]),
];
for (const [from, name] of files) {
  mkdirSync(path.dirname(path.join(directory, name)), { recursive: true });
  copyFileSync(from, path.join(directory, name));
}
writeFileSync(
  path.join(directory, 'NOTICE.txt'),
  'Gifsicle ' +
    engine.version +
    ' (GPL-2.0-only), source SHA256 ' +
    engine.sha256 +
    '\n' +
    'The complete unmodified upstream source is in gifsicle-source.tar.gz.\n' +
    'PixoFold budget/entry wrappers are GPL-2.0-only OR GPL-3.0-or-later; this separate helper is distributed under GPL-2.0-only.\n' +
    'Matching wrapper sources and build recipes are in source/. See source/native/gif/README.md and Gifsicle-COPYING.\n',
);
const hashes = Object.fromEntries(
  [...files.map(([, name]) => name), 'NOTICE.txt'].map((name) => [
    name,
    digest(readFileSync(path.join(directory, name))),
  ]),
);
assert.equal(hashes[binary], identity.helperSha256);
const manifest = {
  schema: 1,
  target,
  binary,
  runtime: process.platform === 'win32' ? 'static-msvc' : 'system',
  engine,
  sources,
  files: hashes,
};
writeFileSync(path.join(directory, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
console.log('GIF桌面资源已准备：' + target + ' / ' + hashes[binary]);
