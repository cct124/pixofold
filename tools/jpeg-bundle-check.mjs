// 无GUI部署验收：复制生产资源和验证程序到系统临时目录，清空开发环境后实际执行。
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync, renameSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { root, target, verifyTools, native, digest } from './jpeg-lab/build.mjs';
import { createSamples, extraMarkers } from './jpeg-lab/experiment.mjs';

const args = process.argv.slice(2);
if (args.length && !(args.length === 2 && args[0] === '--resources' && path.isAbsolute(args[1])))
  throw new Error('参数：可选--resources 已解包的绝对资源根目录');
const resourceSource = args[1] ?? path.join(root, 'src-tauri/resources');
const tools = verifyTools();
native('cargo', [
  'build',
  '--locked',
  '--release',
  '-p',
  'pixofold-desktop',
  '--features',
  'bundle-check',
  '--bin',
  'jpeg-bundle-check',
]);
const extension = process.platform === 'win32' ? '.exe' : '';
const isolated = mkdtempSync(path.join(tmpdir(), 'pixofold-bundle-'));
const install = path.join(isolated, '中文 安装路径');
mkdirSync(install);
cpSync(path.join(resourceSource, 'jpeg'), path.join(install, 'jpeg'), {
  recursive: true,
});
const checker = path.join(install, 'jpeg-bundle-check' + extension);
cpSync(path.join(root, 'target/release/jpeg-bundle-check' + extension), checker);
const sampleDirectory = path.join(isolated, 'samples');
mkdirSync(sampleDirectory);
const { samples } = createSamples({ root, tools, directory: sampleDirectory });
const original = samples.find(([name]) => name === 'baseline-420')[1];
const source = path.join(sampleDirectory, 'input.jpg');
writeFileSync(source, original, { flag: 'wx' });
const cleanEnv =
  process.platform === 'win32'
    ? {
        SystemRoot: process.env.SystemRoot,
        WINDIR: process.env.WINDIR,
        TEMP: isolated,
        TMP: isolated,
        PATH: path.join(process.env.SystemRoot, 'System32'),
      }
    : { TMPDIR: isolated, PATH: '/usr/bin:/bin', LC_ALL: 'C' };
const execute = (file, args, input) => {
  const result = spawnSync(file, args, {
    cwd: isolated,
    env: cleanEnv,
    input,
    timeout: 60000,
    maxBuffer: 64 * 1024 * 1024,
    windowsHide: true,
  });
  if (result.error) throw result.error;
  return result;
};
const output = path.join(isolated, 'outputs');
const run = execute(checker, [install, source, output]);
assert.equal(run.status, 0, run.stderr?.toString());
const report = JSON.parse(run.stdout);
for (const { file } of report.outputs) {
  const bytes = readFileSync(path.join(output, file));
  const decoded = execute(tools.djpeg, ['-strict', '-ppm'], bytes);
  assert.equal(decoded.status, 0, '独立解码失败');
  assert.deepEqual(extraMarkers(bytes), extraMarkers(original));
  if (file === 'lossless.jpg') {
    const before = execute(tools.coefficients, [], original);
    const after = execute(tools.coefficients, [], bytes);
    assert.equal(before.status, 0);
    assert.equal(after.status, 0);
    assert.deepEqual(after.stdout, before.stdout, '无损系数改变');
  }
}
const runtime = path.join(install, 'jpeg/runtime');
const binary = path.join(runtime, 'pixofold-jpeg-helper' + extension);
const helperBytes = readFileSync(binary);
const helperHash = digest(helperBytes);
renameSync(binary, binary + '.saved');
assert.notEqual(execute(checker, [install]).status, 0, '缺工具必须拒绝');
writeFileSync(binary, Buffer.from('untrusted replacement'));
writeFileSync(
  path.join(runtime, 'manifest.json'),
  JSON.stringify({ files: { [path.basename(binary)]: digest(readFileSync(binary)) } }),
);
assert.notEqual(execute(checker, [install]).status, 0, '伪造旁置清单不得授权新工具');
assert.equal(digest(readFileSync(source)), digest(original));
const evidence = mkdtempSync(path.join(root, 'target/jpeg-bundle-'));
cpSync(sampleDirectory, path.join(evidence, 'samples'), { recursive: true });
cpSync(output, path.join(evidence, 'outputs'), { recursive: true });
const evidenceReport = {
  ...report,
  target,
  helperSha256: helperHash,
  isolatedDirectory: isolated,
  missingRejected: true,
  forgedManifestRejected: true,
  sourceUnchanged: true,
  independentOutputsVerified: report.outputs.length,
};
writeFileSync(path.join(evidence, 'report.json'), JSON.stringify(evidenceReport, null, 2) + '\n');
console.log('JPEG随包资源隔离验证通过：' + evidence);
