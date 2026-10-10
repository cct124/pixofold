// 固定源码构建实验程序，不下载到系统或放入产品资源，不改上游源码。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export const root = fileURLToPath(new URL('../..', import.meta.url));
export const engine = JSON.parse(readFileSync(new URL('./engine.json', import.meta.url), 'utf8'));
export const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
export function execute(program, args, options = {}) {
  const result = spawnSync(program, args, {
    cwd: root,
    windowsHide: true,
    timeout: 180000,
    maxBuffer: 16 * 1024 * 1024,
    ...options,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, result.stderr?.toString() || program + '执行失败');
  return result;
}
const targetRun = execute('rustc', ['-vV'], { encoding: 'utf8' });
export const target = targetRun.stdout.match(/^host: (.+)$/m)?.[1];
assert.ok(
  [
    'x86_64-pc-windows-msvc',
    'aarch64-pc-windows-msvc',
    'x86_64-unknown-linux-gnu',
    'aarch64-unknown-linux-gnu',
    'x86_64-apple-darwin',
    'aarch64-apple-darwin',
  ].includes(target),
  '只支持明确的原生目标',
);
const cache = path.join(root, '.tools/gif-lab');
const archive = path.join(cache, 'gifsicle-' + engine.version + '.tar.gz');
const sourceRoot = path.join(cache, 'source-' + engine.sha256.slice(0, 16));
export const source = path.join(sourceRoot, engine.directory);
export const build = path.join(cache, 'build-' + engine.version + '-' + target);
export const binary = path.join(
  build,
  process.platform === 'win32' ? 'Release/gifsicle.exe' : 'src/gifsicle',
);
const definitions = [
  'native/gif/budget.h',
  'native/gif/budget.c',
  'native/gif/entry.c',
  'tools/gif-lab/engine.json',
  'tools/gif-lab/build.mjs',
  'tools/gif-lab/CMakeLists.txt',
];
export const helperDirectory =
  process.platform === 'win32' ? path.join(build, 'Release') : path.join(build, 'helper');
export const helper = path.join(
  helperDirectory,
  'pixofold-gif-helper' + (process.platform === 'win32' ? '.exe' : ''),
);
function tree(directory) {
  return Object.fromEntries(
    readdirSync(directory, { withFileTypes: true })
      .sort((a, b) => a.name.localeCompare(b.name, 'en'))
      .map((entry) => {
        const file = path.join(directory, entry.name);
        assert.ok(entry.isDirectory() || entry.isFile(), '源码含非普通文件');
        return [entry.name, entry.isDirectory() ? tree(file) : digest(readFileSync(file))];
      }),
  );
}
function identity() {
  return {
    engine,
    target,
    platform: process.platform,
    recipe: Object.fromEntries(
      definitions.map((file) => [file, digest(readFileSync(path.join(root, file)))]),
    ),
    sourceTree: tree(source),
    compiler: process.platform === 'win32' ? 'MSVC Release /MT' : 'upstream configure -O2',
    flags: ['no-gifview', 'no-gifdiff', 'no-threads', 'no-simd'],
  };
}
function findCmake(explicit) {
  if (explicit) return explicit;
  if (!spawnSync('cmake', ['--version'], { windowsHide: true }).error) return 'cmake';
  const vswhere = path.join(
    process.env['ProgramFiles(x86)'] ?? '',
    'Microsoft Visual Studio/Installer/vswhere.exe',
  );
  const run = execute(
    vswhere,
    [
      '-latest',
      '-products',
      '*',
      '-find',
      'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe',
    ],
    { encoding: 'utf8' },
  );
  const found = run.stdout.trim().split(String.fromCharCode(10))[0].trim();
  assert.ok(existsSync(found), '缺少CMake');
  return found;
}
export async function buildTools(cmake) {
  mkdirSync(cache, { recursive: true });
  if (!existsSync(archive)) {
    const response = await fetch(engine.archive, { signal: AbortSignal.timeout(120000) });
    assert.ok(response.ok, '官方源码下载失败');
    const bytes = Buffer.from(await response.arrayBuffer());
    assert.equal(digest(bytes), engine.sha256, '源码归档身份不符');
    writeFileSync(archive, bytes, { flag: 'wx' });
  }
  assert.equal(digest(readFileSync(archive)), engine.sha256, '缓存归档身份不符');
  const stamp = path.join(sourceRoot, 'verified.json');
  if (!existsSync(stamp)) {
    assert.ok(!existsSync(source), '源码已存在但无校验标识');
    mkdirSync(sourceRoot, { recursive: true });
    execute('tar', ['-xzf', archive, '-C', sourceRoot]);
    writeFileSync(stamp, JSON.stringify({ sha256: engine.sha256, tree: tree(source) }), {
      flag: 'wx',
    });
  }
  const verified = JSON.parse(readFileSync(stamp, 'utf8'));
  assert.equal(verified.sha256, engine.sha256);
  assert.deepEqual(verified.tree, tree(source), '固定源码缓存被改变');
  mkdirSync(build, { recursive: true });
  if (process.platform === 'win32') {
    const compiler = findCmake(cmake);
    execute(
      compiler,
      [
        '-S',
        path.join(root, 'tools/gif-lab'),
        '-B',
        build,
        '-A',
        target.startsWith('aarch64') ? 'ARM64' : 'x64',
        '-DGIFSICLE_SOURCE=' + source,
      ],
      { stdio: 'inherit' },
    );
    execute(
      compiler,
      ['--build', build, '--config', 'Release', '--target', 'gifsicle', 'pixofold-gif-helper'],
      {
        stdio: 'inherit',
      },
    );
  } else {
    execute(
      path.join(source, 'configure'),
      [
        '--disable-gifview',
        '--disable-gifdiff',
        '--disable-threads',
        '--disable-simd',
        '--disable-dependency-tracking',
      ],
      { cwd: build, env: { ...process.env, CFLAGS: '-O2' }, stdio: 'inherit' },
    );
    execute('make', ['-j2'], { cwd: build, stdio: 'inherit' });
    const compiler = findCmake(cmake);
    execute(
      compiler,
      [
        '-S',
        path.join(root, 'tools/gif-lab'),
        '-B',
        helperDirectory,
        '-DGIFSICLE_SOURCE=' + source,
        '-DGIFSICLE_CONFIG=' + path.join(build, 'config.h'),
        '-DCMAKE_BUILD_TYPE=Release',
      ],
      { stdio: 'inherit' },
    );
    execute(compiler, ['--build', helperDirectory, '--target', 'pixofold-gif-helper', '-j2'], {
      stdio: 'inherit',
    });
  }
  writeFileSync(
    path.join(build, 'identity.json'),
    JSON.stringify(
      {
        ...identity(),
        binarySha256: digest(readFileSync(binary)),
        helperSha256: digest(readFileSync(helper)),
        version: execute(binary, ['--version'], { encoding: 'utf8' }).stdout,
      },
      null,
      2,
    ) + String.fromCharCode(10),
  );
  console.log('GIF固定工具构建完成：' + target);
}
export function verifyTools() {
  const recorded = JSON.parse(readFileSync(path.join(build, 'identity.json'), 'utf8'));
  for (const [key, value] of Object.entries(identity()))
    assert.deepEqual(recorded[key], value, '源码/配方身份过期，请重新构建');
  assert.equal(recorded.binarySha256, digest(readFileSync(binary)), '工具身份改变');
  assert.equal(recorded.helperSha256, digest(readFileSync(helper)), '有界helper身份改变');
  assert.ok(recorded.version.includes(engine.version), '工具版本不符');
  return recorded;
}
