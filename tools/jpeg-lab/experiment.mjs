// 本项目生成的JPEG语料与不变量；所有原始输入和输出留在独立target目录。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { inflateSync } from 'node:zlib';
import { performance } from 'node:perf_hooks';

const maxBuffer = 64 * 1024 * 1024;
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

function execute(program, args, input) {
  return spawnSync(program, args, {
    input,
    maxBuffer,
    timeout: 30000,
    windowsHide: true,
  });
}

function success(program, args, input) {
  const result = execute(program, args, input);
  if (result.error) throw result.error;
  assert.equal(
    result.status,
    0,
    path.basename(program) + ': ' + result.stderr.toString().slice(0, 500),
  );
  return result.stdout;
}

function rejected(program, args, input) {
  const result = execute(program, args, input);
  // 超时/启动错误不是可恢复的输入拒绝，必须单独失败。
  if (result.error) throw result.error;
  assert.ok(
    result.signal === null && [1, 2].includes(result.status),
    '坏输入必须正常拒绝，不能以原生崩溃冒充边界处理',
  );
  return result.stderr.toString().slice(0, 200).trim();
}

function ppm(width, height) {
  const pixels = Buffer.alloc(width * height * 3);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const index = (y * width + x) * 3;
      pixels[index] = (x * 3 + (y % 13) * 7) % 256;
      pixels[index + 1] = (y * 2 + (x % 17) * 5) % 256;
      pixels[index + 2] = (x + y + ((x * y) % 19) * 3) % 256;
    }
  }
  return Buffer.concat([Buffer.from('P6\n' + width + ' ' + height + '\n255\n'), pixels]);
}

function segment(marker, payload) {
  assert.ok(payload.length < 65534);
  const header = Buffer.from([0xff, marker, 0, 0]);
  header.writeUInt16BE(payload.length + 2, 2);
  return Buffer.concat([header, payload]);
}

/** JFIF要求APP0紧随SOI，规范语料在此之后插入元数据。 */
function annotatedJfif(jpeg, annotation) {
  assert.equal(jpeg.readUInt16BE(2), 0xffe0);
  const offset = 4 + jpeg.readUInt16BE(4);
  return Buffer.concat([jpeg.subarray(0, offset), annotation, jpeg.subarray(offset)]);
}

function exif(orientation) {
  const tiff = Buffer.from('49492a0008000000010012010300010000000000000000000000', 'hex');
  tiff.writeUInt16LE(orientation, 18);
  return segment(0xe1, Buffer.concat([Buffer.from('Exif\0\0'), tiff]));
}

function icc(root) {
  const png = readFileSync(path.join(root, 'tests', 'fixtures', 'png', 'icc-rgb8.png'));
  for (let offset = 8; offset < png.length;) {
    const length = png.readUInt32BE(offset);
    if (png.toString('ascii', offset + 4, offset + 8) === 'iCCP') {
      const data = png.subarray(offset + 8, offset + 8 + length);
      const profile = inflateSync(data.subarray(data.indexOf(0) + 2));
      return segment(
        0xe2,
        Buffer.concat([Buffer.from('ICC_PROFILE\0'), Buffer.from([1, 1]), profile]),
      );
    }
    offset += length + 12;
  }
  throw new Error('缺少自生成ICC语料');
}

/** 只枚举完整JPEG marker；熵流中的stuffed字节和restart不作元数据。 */
function markers(jpeg) {
  assert.equal(jpeg.readUInt16BE(0), 0xffd8);
  const found = [];
  let offset = 2;
  while (offset < jpeg.length) {
    if (jpeg[offset] !== 0xff) {
      offset++;
      continue;
    }
    while (jpeg[offset] === 0xff) offset++;
    const marker = jpeg[offset++];
    if (marker === 0 || (marker >= 0xd0 && marker <= 0xd7)) continue;
    if (marker === 0xd9) return found;
    const length = jpeg.readUInt16BE(offset);
    assert.ok(length >= 2 && offset + length <= jpeg.length);
    found.push({ marker, data: jpeg.subarray(offset + 2, offset + length) });
    offset += length;
  }
  throw new Error('JPEG缺少结束标记');
}

export const extraMarkers = (jpeg) =>
  markers(jpeg)
    .filter(({ marker }) => marker === 0xfe || (marker >= 0xe0 && marker <= 0xef))
    .map(({ marker, data }) => marker.toString(16) + ':' + data.toString('hex'));

function dimensions(jpeg) {
  const frame = markers(jpeg).find(({ marker }) => [0xc0, 0xc1, 0xc2].includes(marker));
  assert.ok(frame, '需要受支持的SOF');
  return {
    width: frame.data.readUInt16BE(3),
    height: frame.data.readUInt16BE(1),
    components: frame.data[5],
  };
}

export function createSamples({ root, tools, directory }) {
  const input = path.join(directory, 'original.ppm');
  writeFileSync(input, ppm(192, 128), { flag: 'wx' });
  const base = success(tools.cjpeg, [
    '-revert',
    '-baseline',
    '-quality',
    '90',
    '-sample',
    '2x2',
    input,
  ]);
  const progressive = success(tools.cjpeg, ['-progressive', '-quality', '90', input]);
  const gray = success(tools.cjpeg, [
    '-revert',
    '-baseline',
    '-grayscale',
    '-quality',
    '90',
    input,
  ]);
  const rgb444 = success(tools.cjpeg, ['-baseline', '-quality', '90', '-sample', '1x1', input]);
  const ycbcr422 = success(tools.cjpeg, ['-baseline', '-quality', '90', '-sample', '2x1', input]);
  const rgb = success(tools.cjpeg, ['-baseline', '-rgb', '-quality', '90', input]);
  const cmyk = success(tools.coefficients, ['make-cmyk']);
  const ycck = success(tools.coefficients, ['make-ycck']);
  const oddInput = path.join(directory, 'odd.ppm');
  writeFileSync(oddInput, ppm(193, 127), { flag: 'wx' });
  const odd = success(tools.cjpeg, ['-revert', '-baseline', '-quality', '90', oddInput]);
  const profile = icc(root);
  const profileData = profile.subarray(18);
  const splitAt = Math.floor(profileData.length / 2);
  const splitProfile = Buffer.concat(
    [1, 2].map((index) =>
      segment(
        0xe2,
        Buffer.concat([
          profile.subarray(4, 16),
          Buffer.from([index, 2]),
          index === 1 ? profileData.subarray(0, splitAt) : profileData.subarray(splitAt),
        ]),
      ),
    ),
  );
  const comment = segment(0xfe, Buffer.from('PixoFold self-generated experiment'));
  const protectedMarker = segment(
    0xeb,
    Buffer.from('opaque APP11 synthetic data; no authentic credential'),
  );
  const samples = [
    ['baseline-420', base],
    ['progressive', progressive],
    ['gray', gray],
    ['ycbcr-444', rgb444],
    ['ycbcr-422', ycbcr422],
    ['rgb', rgb],
    ['cmyk', cmyk],
    ['ycck', ycck],
    ['odd-dimensions', odd],
    ['icc', annotatedJfif(base, profile)],
    ['icc-segmented', annotatedJfif(base, splitProfile)],
    ['opaque-app11', annotatedJfif(base, protectedMarker)],
    ['comment', annotatedJfif(base, comment)],
  ];
  for (let orientation = 1; orientation <= 8; orientation++) {
    samples.push(['exif-' + orientation, annotatedJfif(base, exif(orientation))]);
  }
  return { samples, base, progressive, profile };
}

export function runExperiment({ root, engine, buildIdentity, tools }) {
  mkdirSync(path.join(root, 'target'), { recursive: true });
  const directory = mkdtempSync(path.join(root, 'target', 'jpeg-lab-'));
  const { samples, base, progressive, profile } = createSamples({ root, tools, directory });
  const report = {
    engine,
    buildIdentity,
    platform: process.platform,
    arch: process.arch,
    node: process.version,
    toolSha256: Object.fromEntries(
      Object.entries(tools).map(([name, file]) => [name, hash(readFileSync(file))]),
    ),
    samples: [],
    boundaries: {},
  };
  const transcodeArgs = [
    '-copy',
    'all',
    '-optimize',
    '-maxmemory',
    '32m',
    '-maxscans',
    '64',
    '-strict',
  ];
  for (const [name, original] of samples) {
    const source = path.join(directory, name + '.jpg');
    writeFileSync(source, original, { flag: 'wx' });
    const started = performance.now();
    const optimized = success(tools.jpegtran, transcodeArgs, original);
    assert.deepEqual(
      success(tools.coefficients, [], optimized),
      success(tools.coefficients, [], original),
      name + ': 系数/量化/采样必须一致',
    );
    assert.deepEqual(
      extraMarkers(optimized),
      extraMarkers(original),
      name + ': APP/COM必须原字节保持',
    );
    assert.deepEqual(
      success(tools.djpeg, ['-strict', '-ppm', '-maxmemory', '32m'], optimized),
      success(tools.djpeg, ['-strict', '-ppm', '-maxmemory', '32m'], original),
      name + ': 解码像素必须一致',
    );
    writeFileSync(path.join(directory, name + '-lossless.jpg'), optimized, { flag: 'wx' });
    assert.equal(hash(readFileSync(source)), hash(original), '原始输入保持');
    report.samples.push({
      name,
      sourceSha256: hash(original),
      outputSha256: hash(optimized),
      sourceBytes: original.length,
      outputBytes: optimized.length,
      noGain: optimized.length >= original.length,
      losslessMs: Math.round(performance.now() - started),
      ...dimensions(original),
    });
  }
  // q=100仍为重编码；实验量化体积/编码值误差，不把单一误差当作感知画质。
  const decoded = success(tools.djpeg, ['-strict', '-ppm'], base);
  const decodedPath = path.join(directory, 'decoded.ppm');
  writeFileSync(decodedPath, decoded, { flag: 'wx' });
  report.lossy = [];
  for (const quality of [0, 40, 80, 100]) {
    const started = performance.now();
    const output = success(tools.cjpeg, ['-quality', String(quality), decodedPath]);
    const pixels = success(tools.djpeg, ['-strict', '-ppm'], output);
    assert.deepEqual(dimensions(output), dimensions(base));
    const header = decoded.indexOf(Buffer.from('\n255\n')) + 5;
    const outputHeader = pixels.indexOf(Buffer.from('\n255\n')) + 5;
    assert.ok(header >= 5 && outputHeader >= 5);
    const before = decoded.subarray(header);
    const after = pixels.subarray(outputHeader);
    assert.equal(before.length, after.length);
    let sum = 0;
    for (let i = 0; i < before.length; i++) sum += (before[i] - after[i]) ** 2;
    writeFileSync(path.join(directory, 'lossy-q' + quality + '.jpg'), output, { flag: 'wx' });
    report.lossy.push({
      quality,
      bytes: output.length,
      encodingValueRmse: Math.sqrt(sum / before.length),
      elapsedMs: Math.round(performance.now() - started),
      coefficientsEqual: success(tools.coefficients, [], output).equals(
        success(tools.coefficients, [], base),
      ),
    });
  }
  // 方向语料只含本项目生成的Orientation字段；这里验证原像素坐标重编码后保留它。
  // 不据此承诺任意Exif缩略图/私有字段均可安全复制，也不作色彩管理实验。
  report.metadataReencoding = [];
  for (const [name, original] of samples.filter(
    ([name]) => name === 'gray' || name.startsWith('exif-'),
  )) {
    const raster = success(tools.djpeg, ['-strict', '-ppm', '-maxmemory', '32m'], original);
    const rasterPath = path.join(directory, name + '-decoded.ppm');
    writeFileSync(rasterPath, raster, { flag: 'wx' });
    const args = ['-quality', '80'];
    if (name === 'gray') args.push('-grayscale');
    const encoded = success(tools.cjpeg, [...args, rasterPath]);
    const metadata = Buffer.concat(
      markers(original)
        .filter(({ marker }) => marker === 0xe1)
        .map(({ marker, data }) => segment(marker, data)),
    );
    const restored = metadata.length ? annotatedJfif(encoded, metadata) : encoded;
    assert.deepEqual(dimensions(restored), dimensions(original));
    assert.deepEqual(
      extraMarkers(restored),
      extraMarkers(original),
      name + ': 方向/元数据必须保留',
    );
    success(tools.djpeg, ['-strict', '-ppm', '-maxmemory', '32m'], restored);
    writeFileSync(path.join(directory, name + '-reencoded.jpg'), restored, { flag: 'wx' });
    report.metadataReencoding.push({
      name,
      quality: 80,
      bytes: restored.length,
      ...dimensions(restored),
    });
  }
  const annotated = samples.find(([name]) => name === 'icc')[1];
  const defaultCopy = success(tools.jpegtran, ['-optimize'], annotated);
  assert.ok(
    markers(annotated).some(({ marker }) => marker === 0xe2) &&
      !markers(defaultCopy).some(({ marker }) => marker === 0xe2),
    '默认jpegtran参数会丢ICC',
  );
  report.boundaries.defaultCopyDropsIcc = true;
  // 非规范位置的JFIF会被上游重建到前面；显式记录结构变化，不放松规范语料断言。
  const misplaced = Buffer.concat([base.subarray(0, 2), profile, base.subarray(2)]);
  const reordered = success(tools.jpegtran, transcodeArgs, misplaced);
  const originalMarkers = extraMarkers(misplaced);
  assert.deepEqual(extraMarkers(reordered), [originalMarkers[1], originalMarkers[0]]);
  assert.notDeepEqual(extraMarkers(reordered), originalMarkers);
  report.boundaries.jfifMovedBeforeIcc = true;
  writeFileSync(path.join(directory, 'nonstandard-marker-order.jpg'), misplaced, { flag: 'wx' });
  writeFileSync(path.join(directory, 'nonstandard-marker-order-lossless.jpg'), reordered, {
    flag: 'wx',
  });
  const optimized = success(tools.jpegtran, transcodeArgs, base);
  const again = success(tools.jpegtran, transcodeArgs, optimized);
  assert.ok(again.length >= optimized.length, '已优化输入需要无收益保留策略');
  report.boundaries.optimizedInputHasNoGain = true;
  report.boundaries.corruptInput = rejected(tools.coefficients, [], Buffer.from('not a JPEG'));
  report.boundaries.truncatedInput = rejected(
    tools.coefficients,
    [],
    base.subarray(0, Math.floor(base.length / 2)),
  );
  report.boundaries.scanLimit = rejected(
    tools.jpegtran,
    ['-maxscans', '1', '-strict'],
    progressive,
  );
  const oversized = Buffer.from(base);
  const frame = markers(oversized).find(({ marker }) => marker === 0xc0);
  frame.data.writeUInt16BE(16385, 3);
  report.boundaries.dimensionLimit = rejected(tools.coefficients, [], oversized);
  const tooManyPixels = Buffer.from(base);
  const pixelFrame = markers(tooManyPixels).find(({ marker }) => marker === 0xc0);
  pixelFrame.data.writeUInt16BE(4096, 1);
  pixelFrame.data.writeUInt16BE(4096, 3);
  report.boundaries.pixelLimit = rejected(tools.coefficients, [], tooManyPixels);
  const unsupportedPrecision = Buffer.from(base);
  const precisionFrame = markers(unsupportedPrecision).find(({ marker }) => marker === 0xc0);
  precisionFrame.data[0] = 12;
  report.boundaries.unsupportedPrecision = rejected(tools.coefficients, [], unsupportedPrecision);
  const bigInput = path.join(directory, 'large.ppm');
  writeFileSync(bigInput, ppm(1024, 1024), { flag: 'wx' });
  const big = success(tools.cjpeg, ['-revert', '-baseline', '-quality', '90', bigInput]);
  report.boundaries.memoryLimit = rejected(tools.jpegtran, ['-maxmemory', '1m', '-strict'], big);
  writeFileSync(path.join(directory, 'report.json'), JSON.stringify(report, null, 2) + '\n', {
    flag: 'wx',
  });
  console.log(
    'JPEG实验通过：' +
      samples.length +
      '项无损语料、4个有损q锚点、9项灰度/方向重编码及错误/资源拒绝边界。',
  );
  console.log('报告和隔离产物：' + directory);
}
