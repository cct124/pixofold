// 在Rust公开入口上验证真实有损与文件安全，再用独立djpeg/coeffdump复验已提交结果。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { extraMarkers } from './experiment.mjs';

export function runLossyChecks({ root, tools, directory, hash, source }) {
  const execute = (file, args, input, succeeds = true) => {
    const result = spawnSync(file, args, {
      input,
      timeout: 30000,
      maxBuffer: 64 * 1024 * 1024,
      windowsHide: true,
    });
    if (result.error) throw result.error;
    if (succeeds) assert.equal(result.status, 0, 'JPEG有损独立验证失败');
    else assert.notEqual(result.status, 0, '超限操作必须拒绝');
    return result.stdout;
  };
  const base = readFileSync(path.join(directory, 'baseline-420.jpg'));
  const extra = path.join(directory, 'lossy-extra');
  const rejected = path.join(extra, 'rejected');
  mkdirSync(rejected, { recursive: true });
  const save = (name, bytes, folder = directory) =>
    writeFileSync(path.join(folder, name + '.jpg'), bytes, { flag: 'wx' });
  const photo = readFileSync(path.join(source, 'testimages/testorig.jpg'));
  const photoSha256 = 'acc6ec555d41d15b368320edaa3b20958ee6fa97cb6e4a18d1213d5ae8bec73b';
  assert.equal(createHash('sha256').update(photo).digest('hex'), photoSha256, '上游照片夹具身份');
  save('upstream-rose', photo);
  // 固定源码附带的227×149照片只提供最小真实内容观察；证据包保留原始许可/版权。
  for (const notice of ['README.ijg', 'LICENSE.md']) {
    writeFileSync(path.join(extra, notice), readFileSync(path.join(source, notice)), {
      flag: 'wx',
    });
  }
  writeFileSync(
    path.join(extra, 'SOURCE.txt'),
    'MozJPEG v4.1.5, tools/jpeg-lab/engine.json pinned source; testimages/testorig.jpg SHA256 ' +
      photoSha256 +
      '\nOriginal upstream fixture copied unchanged. Compressed derivatives are in lossy-results.\n',
    { flag: 'wx' },
  );
  const jfifEnd = 4 + base.readUInt16BE(4);
  const thumbnail = Buffer.concat([
    base.subarray(0, jfifEnd),
    Buffer.from([0, 0, 0]),
    base.subarray(jfifEnd),
  ]);
  thumbnail.writeUInt16BE(base.readUInt16BE(4) + 3, 4);
  thumbnail[18] = 1;
  thumbnail[19] = 1;
  save('jfif-thumbnail', thumbnail);
  const ambiguous = Buffer.concat([base.subarray(0, 2), base.subarray(jfifEnd)]);
  const frame = ambiguous.indexOf(Buffer.from([0xff, 0xc0]));
  const scan = ambiguous.indexOf(Buffer.from([0xff, 0xda]));
  for (let c = 0; c < 3; c++) {
    ambiguous[frame + 10 + c * 3] += 6;
    ambiguous[scan + 5 + c * 2] += 6;
  }
  save('ambiguous-color', ambiguous);
  const args = ['16384', '16777216', '134217728', '64'];
  save('no-gain', execute(tools.helper, ['lossy', ...args, '1'], base), extra);
  const segment = (marker, payload) => {
    const header = Buffer.from([0xff, marker, 0, 0]);
    header.writeUInt16BE(payload.length + 2, 2);
    return Buffer.concat([base.subarray(0, jfifEnd), header, payload, base.subarray(jfifEnd)]);
  };
  save('unknown-app', segment(0xe3, Buffer.from('unknown')), rejected);
  save('xmp', segment(0xe1, Buffer.from('http://ns.adobe.com/xap/1.0/')), rejected);
  const exif = Buffer.from(readFileSync(path.join(directory, 'exif-6.jpg')));
  const exifAt = exif.indexOf(Buffer.from('Exif'));
  exif[exifAt + 31] = 1;
  save('complex-exif', exif, rejected);
  save('truncated', base.subarray(0, base.length / 2), rejected);
  const scanAt = base.indexOf(Buffer.from([0xff, 0xda]));
  save(
    'bad-entropy',
    Buffer.concat([
      base.subarray(0, scanAt + 2 + base.readUInt16BE(scanAt + 2)),
      Buffer.from([0xff, 0xd9]),
    ]),
    rejected,
  );
  execute(
    tools.helper,
    ['lossy', '16384', '16777216', String(1280 * 1024), '64', '80'],
    base,
    false,
  );
  execute(tools.helper, ['lossy', ...args, '101'], base, false);
  execute(tools.helper, ['lossy', ...args, '0'], base, false);
  const run = spawnSync(
    'cargo',
    [
      'run',
      '--locked',
      '-p',
      'pixofold-core',
      '--example',
      'jpeg_lossy_check',
      '--',
      path.dirname(tools.helper),
      hash,
      directory,
    ],
    { cwd: root, stdio: 'inherit', windowsHide: true, timeout: 180000 },
  );
  if (run.error) throw run.error;
  assert.equal(run.status, 0, 'JPEG真实有损核心/安全输出检查失败');
  const entries = JSON.parse(readFileSync(path.join(directory, 'lossy-results.json'), 'utf8'));
  let outputs = 0;
  for (const entry of entries.filter((entry) => entry.optimized)) {
    const original = readFileSync(path.join(directory, entry.sample + '.jpg'));
    const output = readFileSync(path.join(directory, 'lossy-results', entry.output));
    const before = execute(tools.coefficients, [], original);
    const after = execute(tools.coefficients, [], output);
    assert.deepEqual(after.subarray(0, 20), before.subarray(0, 20), '维度、位深、颜色解释与分量数');
    assert.deepEqual(extraMarkers(output), extraMarkers(original), '元数据原字节与顺序');
    const decoded = execute(tools.djpeg, ['-strict', '-ppm'], output);
    if (entry.fallback) {
      assert.deepEqual(after, before, '回退必须保留系数与实际量化表');
      assert.deepEqual(decoded, execute(tools.djpeg, ['-strict', '-ppm'], original));
    }
    outputs++;
  }
  const qualityEvidence = [];
  const originalPixels = execute(tools.djpeg, ['-strict', '-ppm'], base);
  for (const q of [0, 40, 80, 100]) {
    const candidate = execute(tools.helper, ['lossy', ...args, String(Math.max(q, 1))], base);
    const pixels = execute(tools.djpeg, ['-strict', '-ppm'], candidate);
    assert.equal(pixels.length, originalPixels.length);
    const pixelBytes = 192 * 128 * 3;
    let squared = 0;
    for (let i = pixels.length - pixelBytes; i < pixels.length; i++)
      squared += (pixels[i] - originalPixels[i]) ** 2;
    qualityEvidence.push({
      quality: q,
      candidateBytes: candidate.length,
      rgbRmse: Math.sqrt(squared / pixelBytes),
    });
    save('quality-' + q, candidate, extra);
  }
  assert.ok(
    qualityEvidence[0].rgbRmse > qualityEvidence[3].rgbRmse,
    '固定语料的质量两端须实际区分',
  );
  writeFileSync(
    path.join(directory, 'lossy-report.json'),
    JSON.stringify(
      {
        helperSha256: hash,
        platform: process.platform,
        cases: entries.length,
        outputs,
        qualityEvidence,
        result: 'passed',
        upstreamSample: { file: 'testimages/testorig.jpg', sha256: photoSha256 },
        scope:
          'generated boundary samples and one small upstream photograph; no broad perceptual or ICC calibration claim',
      },
      null,
      2,
    ) + '\n',
    { flag: 'wx' },
  );
  console.log(
    'JPEG有损独立验证：' + entries.length + '组合，' + outputs + '实际输出；记录 ' + directory,
  );
}
