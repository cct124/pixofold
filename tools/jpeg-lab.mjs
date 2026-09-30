// JPEG开发实验入口：固定源码/校验、静态工具构建和隔离语料回归，不开放产品能力。
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { runExperiment } from './jpeg-lab/experiment.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const definition = path.join(root, 'tools', 'jpeg-lab');
const engine = JSON.parse(readFileSync(path.join(definition, 'engine.json'), 'utf8'));
const cache = path.join(root, '.tools', 'jpeg-lab');
const source = path.join(cache, 'mozjpeg-' + engine.commit);
const build = path.join(cache, 'build-' + engine.tag);
const engineBuild = path.join(build, 'engine');
const archive = path.join(cache, 'mozjpeg-' + engine.commit.slice(0, 8) + '.tar.gz');
if (engine.simd !== false) throw new Error('本实验只接受关闭SIMD的固定基线');
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
if (process.platform === 'win32') featureFlags.push('-DWITH_CRT_DLL=ON');
const sourceHash = (file) =>
  createHash('sha256')
    .update(readFileSync(path.join(definition, file)))
    .digest('hex');
const buildIdentity = {
  engine,
  featureFlags,
  validatorSha256: sourceHash('coeffdump.c'),
  validatorCmakeSha256: sourceHash('CMakeLists.txt'),
};
const [command, ...args] = process.argv.slice(2);
let cmake = 'cmake';
if (args.length === 2 && args[0] === '--cmake') cmake = args[1];
else if (args.length !== 0) throw new Error('参数：build [--cmake PATH] 或 check');
if (command !== 'build' && command !== 'check') throw new Error('参数：build 或 check');
if (command === 'check' && args.length !== 0) throw new Error('check无需CMake参数');

function native(program, arguments_) {
  const result = spawnSync(program, arguments_, {
    cwd: root,
    stdio: 'inherit',
    windowsHide: true,
    timeout: 15 * 60 * 1000,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(path.basename(program) + '执行失败');
}

function binaries() {
  const extension = process.platform === 'win32' ? '.exe' : '';
  const resolve = (parent, name) => {
    for (const directory of [parent, path.join(parent, 'Release')]) {
      const candidate = path.join(directory, name + extension);
      if (existsSync(candidate)) return candidate;
    }
    throw new Error('缺少实验工具：' + name + '；先执行jpeg:lab:build');
  };
  return {
    cjpeg: resolve(engineBuild, 'cjpeg-static'),
    djpeg: resolve(engineBuild, 'djpeg-static'),
    jpegtran: resolve(engineBuild, 'jpegtran-static'),
    coefficients: resolve(build, 'pixofold-coeffdump'),
  };
}

if (command === 'build') {
  mkdirSync(cache, { recursive: true });
  if (!existsSync(archive)) {
    const response = await fetch(engine.archive, { signal: AbortSignal.timeout(120000) });
    if (!response.ok) throw new Error('官方源码下载失败：HTTP ' + response.status);
    const bytes = Buffer.from(await response.arrayBuffer());
    if (createHash('sha256').update(bytes).digest('hex') !== engine.sha256)
      throw new Error('官方源码SHA256不匹配');
    writeFileSync(archive, bytes, { flag: 'wx' });
  }
  if (createHash('sha256').update(readFileSync(archive)).digest('hex') !== engine.sha256)
    throw new Error('缓存源码SHA256不匹配；不得继续构建');
  // 解压到新目录，避免合并未知来源的既有源码；缓存身份明确后才复用。
  const stamp = path.join(cache, 'verified-' + engine.commit + '.json');
  if (!existsSync(stamp)) {
    if (existsSync(source)) throw new Error('源码目录已存在但没有校验标识，请检查缓存来源');
    native('tar', ['-xzf', archive, '-C', cache]);
    writeFileSync(stamp, JSON.stringify(engine) + '\n', { flag: 'wx' });
  } else if (JSON.stringify(JSON.parse(readFileSync(stamp, 'utf8'))) !== JSON.stringify(engine)) {
    throw new Error('源码缓存身份不一致');
  }
  native(cmake, [
    '-S',
    source,
    '-B',
    engineBuild,
    ...featureFlags,
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
    '-DCMAKE_BUILD_TYPE=Release',
  ]);
  native(cmake, ['--build', build, '--config', 'Release', '--target', 'pixofold-coeffdump']);
  writeFileSync(path.join(build, 'engine.json'), JSON.stringify(buildIdentity, null, 2) + '\n');
  binaries();
  console.log('JPEG实验工具构建完成：' + engine.tag + ' / ' + engine.commit);
} else {
  const built = JSON.parse(readFileSync(path.join(build, 'engine.json'), 'utf8'));
  if (JSON.stringify(built) !== JSON.stringify(buildIdentity))
    throw new Error('工具版本、构建配置或验证器源码已变化；请重新构建实验工具');
  runExperiment({ root, engine, buildIdentity, tools: binaries() });
}
