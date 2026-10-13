// 显式4A实验：固定有界helper、独立语义/质量比较与隔离输出，结果不开放产品有损能力。
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import {
  root,
  helper,
  helperDirectory,
  source,
  build,
  execute,
  verifyTools,
  digest,
} from './build.mjs';
import { writeFixtures } from './fixtures.mjs';
import { assertCandidate } from './contracts.mjs';

const strengths = ['lossless', '0', '10', '20', '40', '80'];
const readJson = (file) => JSON.parse(readFileSync(file, 'utf8'));
const json = (program, args) => JSON.parse(execute(program, args, { encoding: 'utf8' }).stdout);
export function checkLossySources() {
  const directory = path.join(root, 'tests/fixtures/gif-lossy');
  const provenance = readJson(path.join(directory, 'SOURCES.json'));
  for (const entry of provenance.files) {
    assert.match(entry.file, /^[A-Za-z0-9_.-]+$/);
    assert.equal(digest(readFileSync(path.join(directory, entry.file))), entry.sha256);
  }
  return provenance;
}

// 初轮筛选假设，不是感知评分或产品默认值；全量原始指标保留给人工评估。
function screening(run) {
  if (!run.comparison.hardConstraints) return 'hard-rejected';
  if (run.outputBytes >= run.inputBytes) return 'no-gain';
  if (run.comparison.losslessEquivalent) return 'lossless-gain';
  const passes = [run.comparison.transparent, run.comparison.logical].every(
    (m) =>
      m.rmse <= 2.5 &&
      m.worstIntervalRmse <= 4 &&
      m.maxChannelError <= 64 &&
      m.temporalErrorRmse <= 3 &&
      m.worstTemporalRmse <= 6,
  );
  return passes ? 'quality-candidate' : 'quality-review';
}
const escape = (value) =>
  String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
export function htmlReport(report) {
  const sections = report.samples
    .filter((s) => s.group !== 'structural')
    .map((sample) => {
      const rows = sample.runs
        .map((run) => {
          const m = run.comparison.transparent;
          return (
            '<tr><td>' +
            escape(run.strength) +
            '</td><td>' +
            run.outputBytes +
            '</td><td>' +
            run.encodeMs +
            '</td><td>' +
            (m?.rmse.toFixed(3) ?? '—') +
            '</td><td>' +
            (m?.temporalErrorRmse.toFixed(3) ?? '—') +
            '</td><td>' +
            escape(
              run.screening +
                (run.comparison.rejection ? ' (' + run.comparison.rejection + ')' : ''),
            ) +
            '</td></tr>'
          );
        })
        .join('');
      const views = [
        { label: '输入', file: sample.file },
        ...sample.runs.map((r) => ({
          label: r.strength + (r.comparison.hardConstraints ? '' : ' · 验证拒绝'),
          file: r.file,
        })),
      ]
        .map(
          (item) =>
            '<figure><img loading="lazy" src="' +
            escape(item.file) +
            '" alt="' +
            escape(sample.name + ' ' + item.label) +
            '"><figcaption>' +
            escape(item.label) +
            '</figcaption></figure>',
        )
        .join('');
      return (
        '<section><h2>' +
        escape(sample.name) +
        '</h2><p>' +
        escape(sample.provenance) +
        '</p><div class="views">' +
        views +
        '</div><table><thead><tr><th>原生强度</th><th>输出bytes</th><th>候选ms</th><th>RGB RMSE</th><th>时域误差RMS</th><th>实验筛选</th></tr></thead><tbody>' +
        rows +
        '</tbody></table></section>'
      );
    })
    .join('');
  return (
    '<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>GIF有损实验4A</title><style>body{font:16px/1.6 system-ui,sans-serif;max-width:1440px;margin:32px auto;padding:0 24px;color:#e6edf3;background:#131922}h1,h2{line-height:1.3}section{padding:24px 0;border-top:1px solid #3b4654}.views{display:flex;flex-wrap:wrap;gap:12px}figure{margin:0;padding:8px;background:#273442;border-radius:6px}img{display:block;object-fit:contain;max-width:320px;max-height:320px;min-width:80px;background:repeating-conic-gradient(#ddd 0% 25%,#fff 0% 50%) 0/16px 16px}figcaption{padding:6px}table{border-collapse:collapse;width:100%;margin-top:20px}td,th{text-align:left;padding:8px;border-bottom:1px solid #384553}a{color:#83bcff}code{background:#273442;padding:2px 5px}</style><h1>GIF有损实验4A</h1><p>原图、无损基线、原生lossiness 0/10/20/40/80。浏览器中的GIF不保证同步播放；时间轴正确性由独立验证器核对。先观察小字、透明边缘、运动噪声与循环接缝，再打开文件逐一复查。</p><p>筛选阈值是实验假设；RGB误差是编码值误差，时域量是候选相对原图的误差变化。它们不等于感知质量分数，也不授权生产有损。</p><p><a href="report.json">完整报告与所有结构样本</a> · <a href="sources.json">来源及衍生方式</a></p>' +
    sections +
    '</html>'
  );
}

export function runLossyExperiment() {
  execute(process.execPath, ['--test', path.join(root, 'tools/gif-lab/contracts.test.mjs')], {
    stdio: 'inherit',
  });
  const identity = verifyTools();
  const publicSource = checkLossySources();
  writeFixtures(path.join(root, 'tests/fixtures/gif'), true);
  execute(
    'cargo',
    [
      'build',
      '--release',
      '--locked',
      '-p',
      'pixofold-core',
      '--features',
      'gif-lossy-lab',
      '--example',
      'gif_lossy_lab',
      '--example',
      'gif_validate',
    ],
    { stdio: 'inherit' },
  );
  const suffix = process.platform === 'win32' ? '.exe' : '';
  const lab = path.join(root, 'target/release/examples/gif_lossy_lab' + suffix);
  const validator = path.join(root, 'target/release/examples/gif_validate' + suffix);
  const directory = mkdtempSync(path.join(root, 'target/gif-lossy-'));
  const inputs = path.join(directory, 'inputs');
  const outputs = path.join(directory, 'outputs');
  mkdirSync(inputs);
  mkdirSync(outputs);
  const corpusDir = path.join(root, 'tests/fixtures/gif');
  const corpus = json(validator, ['corpus', corpusDir, path.join(corpusDir, 'manifest.json')]);
  const samples = [];
  const skipped = [];
  const add = (file, name, group, provenance) => {
    const inspected = json(validator, ['inspect', file]);
    if (inspected.result !== 'accepted') {
      skipped.push({ name, group, ...inspected });
      return;
    }
    const destination = path.join(inputs, name);
    if (file !== destination) cpSync(file, destination, { errorOnExist: true, force: false });
    samples.push({
      name,
      group,
      provenance,
      file: 'inputs/' + name,
      sha256: digest(readFileSync(destination)),
      runs: [],
    });
  };
  for (const item of readJson(path.join(corpusDir, 'manifest.json')).samples.filter(
    (s) => s.expect === 'ok',
  ))
    add(
      path.join(corpusDir, item.file),
      'corpus-' + item.file,
      'structural',
      '本项目确定性GIF语料，人工语义预期',
    );
  const manualDir = path.join(root, 'tests/manual-images');
  const manual = readJson(path.join(manualDir, 'SOURCES.json'));
  for (const item of manual.files.filter((s) => s.group === 'GIF')) {
    const file = path.join(manualDir, item.file);
    assert.equal(digest(readFileSync(file)), item.sha256);
    add(
      file,
      'public-' + path.basename(item.file),
      'public',
      item.source_page + ' · ' + item.license,
    );
  }
  json(lab, ['fixtures', inputs, path.join(manualDir, 'gif/hopper.gif')]);
  for (const name of [
    'gradient-motion',
    'transparent-illustration',
    'transparent-seam',
    'ui-motion',
    'photo-pan',
  ])
    add(
      path.join(inputs, name + '.gif'),
      name + '.gif',
      'generated',
      name === 'photo-pan'
        ? '由公开Pillow hopper.gif裁出96×96、12帧平移，继承原图许可；输入GIF由gif编码器生成'
        : name === 'transparent-seam'
          ? '本项目三像素/两帧循环：完整透明帧恢复背景，验证最后处置变化导致的第二轮残留。'
          : '本项目整数规则生成12帧动画，GPL-3.0-or-later；ui-motion为合成UI，不是真实录屏',
    );
  const screen = path.join(root, 'tests/fixtures/gif-lossy/openrefine-wikidata.gif');
  const originalScreen = json(validator, ['inspect', screen]);
  assert.equal(originalScreen.code, 'ResourceLimit');
  skipped.push({ name: 'openrefine-original', ...originalScreen });
  const derived = path.join(inputs, 'openrefine-320.gif');
  execute(
    helper,
    [
      '--memory-bytes',
      String(32 * 1024 * 1024),
      '--careful',
      '-O2',
      '--resize-fit',
      '320x320',
      '--output',
      derived,
      screen,
    ],
    { timeout: 30000 },
  );
  add(
    derived,
    'openrefine-320.gif',
    'derived',
    'Pintoch真实录屏，CC BY-SA 4.0；固定helper按320×320包围框缩至320×309，所有收益以此衍生输入为基准。',
  );
  const sourceRecord = {
    publicSource,
    manualSources: manual.files.filter((s) => s.group === 'GIF'),
    derivedRecordingRecipe: [
      '--memory-bytes',
      '33554432',
      '--careful',
      '-O2',
      '--resize-fit',
      '320x320',
    ],
    photoRecipe:
      'gif_lab/quality_fixtures.rs; 96x96 crop translated by 2 pixels over 12 frames from 128x128 hopper.gif',
  };
  writeFileSync(path.join(directory, 'sources.json'), JSON.stringify(sourceRecord, null, 2));
  const report = {
    schema: 'pixofold.gif-lossy-lab.v2',
    result: 'in_progress',
    nativeVersion: identity.version,
    helperSha256: identity.helperSha256,
    target: identity.target,
    strengths,
    corpus,
    skipped,
    samples,
    qualityGate: {
      label: 'experimental screening only; not production acceptance',
      temporalNormalization:
        'RMS over error-changing boundaries; identical-error splits do not add a denominator',
      rmse: 2.5,
      worstIntervalRmse: 4,
      maxChannelError: 64,
      temporalErrorRmse: 3,
      worstTemporalRmse: 6,
    },
    summary: [],
  };
  for (const sample of samples) {
    for (const strength of strengths) {
      const file = 'outputs/' + strength + '-' + sample.name;
      const run = json(lab, [
        helperDirectory,
        identity.helperSha256,
        path.join(directory, sample.file),
        path.join(directory, file),
        strength,
      ]);
      assertCandidate(sample, strength, run);
      sample.runs.push({ strength, file, ...run, screening: screening(run) });
      assert.equal(digest(readFileSync(path.join(directory, sample.file))), sample.sha256);
    }
    console.log('GIF有损实验已测：' + sample.name);
    writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2));
  }
  for (const strength of strengths) {
    const rows = samples.map((s) => s.runs.find((r) => r.strength === strength));
    report.summary.push({
      strength,
      cases: rows.length,
      hardRejected: rows.filter((r) => !r.comparison.hardConstraints).length,
      candidateGains: rows.filter((r) => r.screening === 'quality-candidate').length,
      losslessGains: rows.filter((r) => r.screening === 'lossless-gain').length,
      noGain: rows.filter((r) => r.screening === 'no-gain').length,
      qualityReview: rows.filter((r) => r.screening === 'quality-review').length,
    });
  }
  cpSync(path.join(build, 'identity.json'), path.join(directory, 'engine-identity.json'));
  for (const notice of ['COPYING', 'README.md'])
    cpSync(path.join(source, notice), path.join(directory, 'Gifsicle-' + notice));
  cpSync(path.join(manualDir, 'licenses'), path.join(directory, 'sample-licenses'), {
    recursive: true,
  });
  cpSync(
    path.join(root, 'tests/fixtures/gif-lossy/CC-BY-SA-4.0.txt'),
    path.join(directory, 'sample-licenses/CC-BY-SA-4.0.txt'),
  );
  for (const item of manual.files.filter((s) => s.group === 'GIF')) {
    assert.equal(
      digest(readFileSync(path.join(manualDir, item.file))),
      item.sha256,
      '公开原件变化',
    );
  }
  checkLossySources();
  report.result = 'measured';
  writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2));
  writeFileSync(path.join(directory, 'index.html'), htmlReport(report));
  console.log(
    JSON.stringify({
      directory,
      samples: samples.length,
      cases: samples.length * strengths.length,
      summary: report.summary,
    }),
  );
}
