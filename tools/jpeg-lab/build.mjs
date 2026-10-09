// 实验和桌面共用固定源码与构建配方；下载/编译只在显式prepare/build时执行。
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export const root = fileURLToPath(new URL('../..', import.meta.url));
const definition = path.join(root, 'tools', 'jpeg-lab');
export const engine = JSON.parse(readFileSync(path.join(definition, 'engine.json'), 'utf8'));
export const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
export const sourceFiles = [
  'native/jpeg/helper.c',
  'tools/jpeg-lab/build.mjs',
  'tools/jpeg-lab/CMakeLists.txt',
  'tools/jpeg-lab/engine.json',
  'tools/jpeg-bundle.mjs',
];

const rust = spawnSync('rustc', ['-vV'], { encoding: 'utf8', windowsHide: true });
if (rust.error) throw rust.error;
export const target = rust.stdout.match(/^host: (.+)$/m)?.[1];
const supported = [
  'x86_64-pc-windows-msvc',
  'aarch64-pc-windows-msvc',
  'x86_64-unknown-linux-gnu',
  'aarch64-unknown-linux-gnu',
  'x86_64-apple-darwin',
  'aarch64-apple-darwin',
];
if (rust.status !== 0 || !supported.includes(target))
  throw new Error('JPEG构建仅支持明确的原生Rust目标，不隐式交叉编译');

const cache = path.join(root, '.tools', 'jpeg-lab');
const sourceCache = path.join(cache, 'source-' + engine.sha256.slice(0, 16));
export const source = path.join(sourceCache, 'mozjpeg-' + engine.commit);
export const build = path.join(cache, 'build-' + engine.tag + '-' + target + '-static-v2');
const engineBuild = path.join(build, 'engine');
const archive = path.join(cache, 'mozjpeg-' + engine.commit.slice(0, 8) + '.tar.gz');
if (engine.simd !== false) throw new Error('当前配方只接受关闭SIMD的固定基线');
const featureFlags = [
  '-DENABLE_SHARED=OFF',
  '-DENABLE_STATIC=ON',
  '-DWITH_SIMD=OFF',
  '-DWITH_TURBOJPEG=OFF',
  '-DWITH_JAVA=OFF',
  '-DPNG_SUPPORTED=OFF',
  '-DWITH_12BIT=OFF',
  '-DWITH_ARITH_DEC=OFF',
  '-DWITH_ARITH_ENC=OFF',
  '-DWITH_JPEG7=OFF',
  '-DWITH_JPEG8=OFF',
  '-DWITH_MEM_SRCDST=ON',
  '-DBUILD=' + engine.commit.slice(0, 12),
];
if (process.platform === 'win32') featureFlags.push('-DWITH_CRT_DLL=OFF');
export const buildIdentity = {
  engine,
  target,
  runtime: process.platform === 'win32' ? 'static-msvc' : 'system',
  featureFlags,
  sources: Object.fromEntries(
    sourceFiles.map((file) => [file, digest(readFileSync(path.join(root, file)))]),
  ),
  validatorSha256: digest(readFileSync(path.join(definition, 'coeffdump.c'))),
};

export function native(program, arguments_, options = {}) {
  const result = spawnSync(program, arguments_, {
    cwd: root,
    stdio: 'inherit',
    windowsHide: true,
    timeout: 15 * 60 * 1000,
    ...options,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(path.basename(program) + '执行失败');
  return result;
}

export function binaries() {
  const extension = process.platform === 'win32' ? '.exe' : '';
  const resolve = (parent, name) => {
    for (const directory of [parent, path.join(parent, 'Release')]) {
      const candidate = path.join(directory, name + extension);
      if (existsSync(candidate)) return candidate;
    }
    throw new Error('缺少工具：' + name + '；先执行jpeg:lab:build');
  };
  return {
    cjpeg: resolve(engineBuild, 'cjpeg-static'),
    djpeg: resolve(engineBuild, 'djpeg-static'),
    jpegtran: resolve(engineBuild, 'jpegtran-static'),
    coefficients: resolve(build, 'pixofold-coeffdump'),
    helper: resolve(build, 'pixofold-jpeg-helper'),
  };
}

function findCmake(explicit) {
  if (explicit) return explicit;
  if (process.env.CMAKE) return process.env.CMAKE;
  if (!spawnSync('cmake', ['--version'], { windowsHide: true }).error) return 'cmake';
  if (process.platform === 'win32' && process.env['ProgramFiles(x86)']) {
    const vswhere = path.join(
      process.env['ProgramFiles(x86)'],
      'Microsoft Visual Studio',
      'Installer',
      'vswhere.exe',
    );
    const found = spawnSync(
      vswhere,
      [
        '-latest',
        '-products',
        '*',
        '-find',
        'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe',
      ],
      { encoding: 'utf8', windowsHide: true },
    );
    const candidate = found.status === 0 ? found.stdout.trim().split(/\r?\n/)[0] : '';
    if (candidate && existsSync(candidate)) return candidate;
  }
  throw new Error('缺少CMake；请安装或通过CMAKE/--cmake指定');
}

export async function buildTools(cmakePath) {
  const cmake = findCmake(cmakePath);
  mkdirSync(cache, { recursive: true });
  if (!existsSync(archive)) {
    const response = await fetch(engine.archive, { signal: AbortSignal.timeout(120000) });
    if (!response.ok) throw new Error('官方源码下载失败：HTTP ' + response.status);
    const bytes = Buffer.from(await response.arrayBuffer());
    if (digest(bytes) !== engine.sha256) throw new Error('官方源码SHA256不匹配');
    writeFileSync(archive, bytes, { flag: 'wx' });
  }
  if (digest(readFileSync(archive)) !== engine.sha256) throw new Error('缓存源码SHA256不匹配');
  const stamp = path.join(sourceCache, 'verified.json');
  if (!existsSync(stamp)) {
    if (existsSync(source)) throw new Error('源码目录已存在但没有校验标识，请检查缓存来源');
    mkdirSync(sourceCache, { recursive: true });
    native('tar', ['-xzf', archive, '-C', sourceCache]);
    writeFileSync(stamp, JSON.stringify({ engine, tree: sourceTree(source) }) + '\n', {
      flag: 'wx',
    });
  } else {
    const verified = JSON.parse(readFileSync(stamp, 'utf8'));
    if (
      JSON.stringify(verified.engine) !== JSON.stringify(engine) ||
      JSON.stringify(verified.tree) !== JSON.stringify(sourceTree(source))
    )
      throw new Error('源码缓存身份/内容已变化；请检查来源后使用新的缓存，不能继续构建');
  }
  const platformFlags =
    process.platform === 'win32'
      ? [
          '-A',
          target.startsWith('aarch64-') ? 'ARM64' : 'x64',
          '-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded',
        ]
      : process.platform === 'darwin'
        ? ['-DCMAKE_OSX_ARCHITECTURES=' + (target.startsWith('aarch64-') ? 'arm64' : 'x86_64')]
        : [];
  native(cmake, [
    '-S',
    source,
    '-B',
    engineBuild,
    ...featureFlags,
    ...platformFlags,
    '-DCMAKE_BUILD_TYPE=Release',
    '-DCMAKE_POLICY_VERSION_MINIMUM=3.5',
  ]);
  native(cmake, [
    '--build',
    engineBuild,
    '--config',
    'Release',
    '--parallel',
    '2',
    '--target',
    'cjpeg-static',
    'djpeg-static',
    'jpegtran-static',
  ]);
  native(cmake, [
    '-S',
    definition,
    '-B',
    build,
    '-DMOZJPEG_SOURCE=' + source,
    '-DMOZJPEG_BUILD=' + engineBuild,
    ...platformFlags,
    '-DCMAKE_BUILD_TYPE=Release',
  ]);
  native(cmake, [
    '--build',
    build,
    '--config',
    'Release',
    '--target',
    'pixofold-coeffdump',
    'pixofold-jpeg-helper',
  ]);
  writeFileSync(path.join(build, 'engine.json'), JSON.stringify(buildIdentity, null, 2) + '\n');
  writeFileSync(path.join(build, 'helper.sha256'), digest(readFileSync(binaries().helper)) + '\n');
  console.log('JPEG工具构建完成：' + engine.tag + ' / ' + target);
}

export function verifyTools() {
  const built = JSON.parse(readFileSync(path.join(build, 'engine.json'), 'utf8'));
  if (JSON.stringify(built) !== JSON.stringify(buildIdentity))
    throw new Error('工具版本、构建配置或源码已变化；请重新构建');
  const tools = binaries();
  if (
    digest(readFileSync(tools.helper)) !==
    readFileSync(path.join(build, 'helper.sha256'), 'utf8').trim()
  )
    throw new Error('工具产物SHA256已变化；请重新构建');
  return tools;
}

function sourceTree(directory) {
  return readdirSync(directory, { withFileTypes: true })
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
    .map((entry) => {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) return [entry.name, sourceTree(file)];
      if (entry.isFile()) return [entry.name, digest(readFileSync(file))];
      throw new Error('源码缓存包含链接或特殊文件');
    });
}
