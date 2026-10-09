// 显式照片/大图验收：实际桌面调度、宿主/helper工作集和独立JPEG输出检查。
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir, cpus, totalmem } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { root, target, verifyTools, native, digest } from './jpeg-lab/build.mjs';
import { extraMarkers } from './jpeg-lab/experiment.mjs';

const options = new Map();
const args = process.argv.slice(2);
for (let i = 0; i < args.length; i += 2) {
  assert.ok(
    ['--inputs', '--resources', '--stress', '--expectations'].includes(args[i]),
    '未知参数',
  );
  assert.ok(args[i + 1] && path.isAbsolute(args[i + 1]), '参数须为绝对路径');
  assert.ok(!options.has(args[i]), '参数重复');
  options.set(args[i], args[i + 1]);
}
assert.ok(
  options.has('--inputs'),
  '参数：--inputs 隔离语料目录 [--resources 资源目录] [--stress 并发语料目录]',
);
const tools = verifyTools();
const expectations = options.has('--expectations')
  ? JSON.parse(readFileSync(options.get('--expectations'), 'utf8'))
  : {};
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
const isolated = mkdtempSync(path.join(tmpdir(), 'pixofold-photo-'));
const install = path.join(isolated, '中文 安装路径');
mkdirSync(install);
cpSync(
  path.join(options.get('--resources') ?? path.join(root, 'src-tauri/resources'), 'jpeg'),
  path.join(install, 'jpeg'),
  { recursive: true },
);
const extension = process.platform === 'win32' ? '.exe' : '';
const checker = path.join(install, 'jpeg-bundle-check' + extension);
cpSync(path.join(root, 'target/release/jpeg-bundle-check' + extension), checker);
const evidence = mkdtempSync(path.join(root, 'target/jpeg-acceptance-'));
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
const execute = (file, args, input, deployment = false) => {
  const run = spawnSync(file, args, {
    cwd: isolated,
    env: deployment ? cleanEnv : process.env,
    input,
    windowsHide: true,
    timeout: 360000,
    maxBuffer: 64 * 1024 * 1024,
  });
  if (run.error) throw run.error;
  assert.equal(run.status, 0, run.stderr?.toString());
  return run.stdout;
};
function ppm(bytes) {
  assert.deepEqual(bytes.subarray(0, 3), Buffer.from([80, 54, 10]), '独立解码PPM标识无效');
  const headerEnd = bytes.indexOf(Buffer.from([10, 50, 53, 53, 10]));
  assert.ok(headerEnd > 3 && headerEnd < 100, '独立解码PPM头无效');
  const dimensions = bytes.subarray(3, headerEnd).toString('ascii').split(' ');
  const width = Number(dimensions[0]);
  const height = Number(dimensions[1]);
  assert.ok(Number.isSafeInteger(width) && width > 0 && Number.isSafeInteger(height) && height > 0);
  const pixels = bytes.subarray(headerEnd + 5);
  assert.equal(pixels.length, width * height * 3, '独立像素长度无效');
  return { width, height, pixels };
}
function metrics(before, after) {
  assert.equal(after.width, before.width);
  assert.equal(after.height, before.height);
  let squared = 0;
  let absolute = 0;
  for (let i = 0; i < before.pixels.length; i++) {
    const error = before.pixels[i] - after.pixels[i];
    squared += error * error;
    absolute += Math.abs(error);
  }
  const mse = squared / before.pixels.length;
  return {
    width: before.width,
    height: before.height,
    rmse: Math.sqrt(mse),
    meanAbsoluteError: absolute / before.pixels.length,
    psnrDb: mse === 0 ? null : 10 * Math.log10((255 * 255) / mse),
    exactPixels: mse === 0,
  };
}
function round(label, inputs, mode) {
  const sources = readdirSync(inputs, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => ({
      file: entry.name,
      sha256: digest(readFileSync(path.join(inputs, entry.name))),
    }));
  const output = path.join(evidence, label);
  const report = JSON.parse(
    execute(checker, ['--profile', install, inputs, output, mode], undefined, true),
  );
  assert.equal(report.shutdown, 'completed');
  const expected = expectations[label] ?? {};
  for (const item of [...report.jobs, ...report.scanIssues]) {
    if (Object.hasOwn(expected, item.file)) {
      assert.equal(item.errorCode ?? item.code, expected[item.file], '预期拒绝类别不符');
    } else {
      assert.ok(
        item.state === 'succeeded' || item.state === 'no_gain',
        '照片存在非预期处理或扫描失败',
      );
    }
  }
  for (const file of Object.keys(expected))
    assert.ok(
      sources.some((source) => source.file === file),
      '缺少预期边界样本',
    );
  assert.equal(report.cancelled, 0);
  assert.ok(report.sampledPeakActive <= report.configuredWorkers);
  assert.ok(report.sampledPeakReservedBytes <= report.workingSetBudgetBytes);
  const independent = [];
  for (const job of report.jobs) {
    const destination = path.join(output, 'results', job.file);
    if (job.state !== 'succeeded') {
      assert.ok(
        job.state === 'no_gain' || (job.state === 'failed' && job.errorCode === 'ResourceLimit'),
      );
      assert.ok(!readdirSync(path.dirname(destination)).includes(job.file), '未提交项产生意外副本');
      continue;
    }
    const source = readFileSync(path.join(inputs, job.file));
    const candidate = readFileSync(destination);
    assert.ok(candidate.length < source.length);
    if (job.format !== 'jpeg') continue;
    const before = ppm(execute(tools.djpeg, ['-strict', '-ppm'], source));
    const after = ppm(execute(tools.djpeg, ['-strict', '-ppm'], candidate));
    assert.deepEqual(extraMarkers(candidate), extraMarkers(source), '元数据改变');
    const measurement = metrics(before, after);
    if (job.lossless) {
      assert.equal(measurement.exactPixels, true, '无损像素改变');
      assert.deepEqual(
        execute(tools.coefficients, [], candidate),
        execute(tools.coefficients, [], source),
        '无损系数/量化表改变',
      );
    }
    independent.push({
      file: job.file,
      sourceSha256: digest(source),
      outputSha256: digest(candidate),
      sourceBytes: source.length,
      outputBytes: candidate.length,
      ...measurement,
    });
  }
  assert.equal(
    readdirSync(path.join(output, 'results')).length,
    report.succeeded,
    '输出有未清理产物',
  );
  for (const source of sources)
    assert.equal(digest(readFileSync(path.join(inputs, source.file))), source.sha256, '输入被修改');
  const accounted = [...report.jobs, ...report.scanIssues].map((item) => item.file).sort();
  assert.deepEqual(
    accounted,
    sources.map((item) => item.file).sort(),
    '输入未完整形成任务或扫描反馈',
  );
  const result = { ...report, mode, independent, sources, sourceUnchanged: true };
  writeFileSync(
    path.join(output, 'verified.json'),
    JSON.stringify(result, null, 2) + String.fromCharCode(10),
  );
  console.log(
    JSON.stringify({
      label,
      succeeded: report.succeeded,
      noGain: report.noGain,
      scanRejected: report.scanIssues.length,
      processingMs: report.processingMs,
      combinedRssMiB: report.sampledPeakCombinedRssBytes / 1024 / 1024,
    }),
  );
  return result;
}
const rounds = ['lossless', '80', '40', '100'].map((mode) =>
  round('mode-' + mode, options.get('--inputs'), mode),
);
if (options.has('--stress')) rounds.push(round('stress-80', options.get('--stress'), '80'));
const report = {
  result: 'passed',
  target,
  checkerSha256: digest(readFileSync(checker)),
  helperSha256: digest(
    readFileSync(path.join(install, 'jpeg/runtime/pixofold-jpeg-helper' + extension)),
  ),
  logicalCpus: cpus().length,
  cpuModel: cpus()[0]?.model,
  totalMemoryBytes: totalmem(),
  samplingScope:
    'release desktop task backend + direct JPEG helpers; no WebView/GUI; sampled working set is not a hard limit',
  perceptualScope: 'PSNR/MAE and sample inspection only; no universal quality threshold',
  rounds,
};
writeFileSync(
  path.join(evidence, 'report.json'),
  JSON.stringify(report, null, 2) + String.fromCharCode(10),
);
console.log('JPEG照片/大图验收通过：' + evidence);
