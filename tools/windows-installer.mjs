import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { findCmake, native, root, target } from './jpeg-lab/build.mjs';

// Windows原生卸载动作仅嵌入MSI；不安装通用路径删除命令，不随其他平台应用分发。
const command = process.argv[2];
if (!['prepare', 'check'].includes(command))
  throw new Error('Usage: windows-installer.mjs prepare|check');
if (process.platform !== 'win32') {
  console.log('Windows MSI actions: not applicable on this platform.');
} else {
  const config = JSON.parse(readFileSync(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
  if (config.identifier !== 'app.pixofold.desktop')
    throw new Error(
      'Update the native installer data scope when changing the application identifier.',
    );
  const architectures = { 'x86_64-pc-windows-msvc': 'x64', 'aarch64-pc-windows-msvc': 'ARM64' };
  const architecture = architectures[target];
  if (!architecture) throw new Error('Unsupported Windows MSI target: ' + target);
  const build = path.join(root, 'target/windows-installer', target);
  const runtime = path.join(root, 'src-tauri/windows/runtime');
  const cmake = findCmake();
  const checking = command === 'check';
  native(cmake, [
    '-S',
    path.join(root, 'native/windows-installer'),
    '-B',
    build,
    '-A',
    architecture,
    '-DPFX_BUILD_TESTS=' + (checking ? 'ON' : 'OFF'),
  ]);
  native(cmake, ['--build', build, '--config', 'Release', '--parallel', '2']);
  const dll = path.join(build, 'Release/pixofold-installer.dll');
  mkdirSync(runtime, { recursive: true });
  const name = 'pixofold-installer-' + architecture.toLowerCase() + '.dll';
  copyFileSync(dll, path.join(runtime, name));
  const sha256 = createHash('sha256').update(readFileSync(dll)).digest('hex');
  writeFileSync(
    path.join(runtime, 'manifest.json'),
    JSON.stringify({ target, file: name, sha256 }, null, 2) + '\n',
  );
  if (checking) {
    const evidence = mkdtempSync(path.join(root, 'target/installer-cleanup-'));
    native(path.join(build, 'Release/cleanup-tests.exe'), [evidence]);
    writeFileSync(
      path.join(evidence, 'result.json'),
      JSON.stringify({ result: 'passed', target, dllSha256: sha256 }, null, 2) + '\n',
    );
    console.log('Windows installer cleanup evidence: ' + evidence);
  }
  console.log('Windows MSI actions prepared: ' + target + ' / ' + sha256);
}
