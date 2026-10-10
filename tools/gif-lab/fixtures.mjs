// 独立小型GIF编码器与人工画布oracle，不使用Gifsicle或Rust解码器生成预期结果。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const colors = {
  K: [0, 0, 0],
  R: [255, 0, 0],
  G: [0, 255, 0],
  B: [0, 0, 255],
  W: [255, 255, 255],
  '.': [0, 0, 0],
};
const u16 = (value) => Buffer.from([value & 255, value >> 8]);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
function subblocks(bytes) {
  const blocks = [];
  for (let at = 0; at < bytes.length; at += 255) {
    const part = bytes.subarray(at, at + 255);
    blocks.push(Buffer.from([part.length]), part);
  }
  return Buffer.concat([...blocks, Buffer.from([0])]);
}
function palette(symbols) {
  assert.ok(symbols.length >= 2 && (symbols.length & (symbols.length - 1)) === 0);
  return Buffer.from(symbols.flatMap((symbol) => colors[symbol]));
}
function lzw(indices, minimum, includeEnd = true) {
  // 每个像素前清字典，固定宽度码；冗余编码让结构/熵优化有真实收益。
  const bits = minimum + 1;
  const clear = 1 << minimum;
  const codes = indices.flatMap((index) => [clear, index]);
  if (includeEnd) codes.push(clear + 1);
  const output = Buffer.alloc(Math.ceil((codes.length * bits) / 8));
  let at = 0;
  for (const code of codes) {
    for (let bit = 0; bit < bits; bit++, at++) output[at >> 3] |= ((code >> bit) & 1) << (at & 7);
  }
  return output;
}
function frame(rows, extra = {}) {
  return { rows, left: 0, top: 0, delayCs: 5, dispose: 1, ...extra };
}
function encode(scene) {
  const global = scene.global ?? ['K', 'R', 'G', 'B'];
  const chunks = [
    Buffer.from(scene.version ?? 'GIF89a'),
    u16(scene.width),
    u16(scene.height),
    Buffer.from([
      global.length ? 0x80 | 0x70 | (Math.log2(global.length) - 1) : 0,
      scene.background ?? 0,
      0,
    ]),
  ];
  if (global.length) chunks.push(palette(global));
  if (scene.loop !== undefined)
    chunks.push(
      Buffer.from([0x21, 0xff, 11]),
      Buffer.from('NETSCAPE2.0'),
      Buffer.from([3, 1]),
      u16(scene.loop),
      Buffer.from([0]),
    );
  if (scene.comment) chunks.push(Buffer.from([0x21, 0xfe]), subblocks(Buffer.from(scene.comment)));
  if (scene.application)
    chunks.push(
      Buffer.from([0x21, 0xff, 11]),
      Buffer.from(scene.application),
      subblocks(Buffer.from([1, 2, 3])),
    );
  for (const item of scene.frames) {
    if (scene.version !== 'GIF87a')
      chunks.push(
        Buffer.from([
          0x21,
          0xf9,
          4,
          (item.dispose << 2) | (item.transparent ? 1 : 0) | (item.userInput ? 2 : 0),
        ]),
        u16(item.delayCs),
        Buffer.from([0, 0]),
      );
    const active = item.palette ?? global;
    const width = item.rows[0].length;
    const height = item.rows.length;
    assert.ok(item.rows.every((row) => row.length === width));
    const local = item.palette ? 0x80 | (Math.log2(active.length) - 1) : 0;
    chunks.push(
      Buffer.from([0x2c]),
      u16(item.left),
      u16(item.top),
      u16(width),
      u16(height),
      Buffer.from([local | (item.interlaced ? 0x40 : 0)]),
    );
    if (item.palette) chunks.push(palette(active));
    const indices = [];
    let order = Array.from({ length: height }, (_, y) => y);
    if (item.interlaced)
      order = [
        [0, 8],
        [4, 8],
        [2, 4],
        [1, 2],
      ].flatMap(([start, step]) =>
        Array.from(
          { length: Math.max(0, Math.ceil((height - start) / step)) },
          (_, n) => start + n * step,
        ),
      );
    for (const y of order)
      for (const symbol of item.rows[y]) indices.push(symbol === '.' ? 0 : active.indexOf(symbol));
    assert.ok(indices.every((index) => index >= 0));
    const minimum = Math.max(2, Math.log2(Math.max(2, active.length)));
    chunks.push(Buffer.from([minimum]), subblocks(lzw(indices, minimum, !item.omitEnd)));
  }
  return Buffer.concat([...chunks, Buffer.from([0x3b])]);
}
function visible(rows, delayCs) {
  const rgba = Buffer.from(
    rows.flatMap((row) =>
      [...row].flatMap((symbol) => [...colors[symbol], symbol === '.' ? 0 : 255]),
    ),
  );
  return { delayCs, sha256: hash(rgba) };
}
export function createFixtures() {
  const files = new Map();
  const entries = [];
  const comparisons = [];
  function add(name, scene, transparent, logical = transparent, extra = {}) {
    const bytes = encode(scene);
    const { secondCycle, ...sampleDetails } = extra;
    const expected = transparent
      ? {
          width: scene.width,
          height: scene.height,
          loopCount: scene.loop ?? null,
          commentHex: scene.comment ? [Buffer.from(scene.comment).toString('hex')] : [],
          transparent: transparent.map(([rows, delay]) => visible(rows, delay)),
          logical: logical.map(([rows, delay]) => visible(rows, delay)),
          ...(secondCycle
            ? {
                transparentSecondCycle: secondCycle.transparent.map(([rows, delay]) =>
                  visible(rows, delay),
                ),
                logicalSecondCycle: secondCycle.logical.map(([rows, delay]) =>
                  visible(rows, delay),
                ),
              }
            : {}),
        }
      : undefined;
    files.set(name + '.gif', bytes);
    entries.push({
      file: name + '.gif',
      sha256: hash(bytes),
      expect: 'ok',
      ...sampleDetails,
      ...(expected ? { expected } : {}),
    });
    return bytes;
  }
  function reject(name, bytes, code = 'InvalidGif', limits) {
    files.set(name + '.gif', bytes);
    entries.push({
      file: name + '.gif',
      sha256: hash(bytes),
      expect: code,
      ...(limits ? { limits } : {}),
    });
  }
  const red = ['RRR', 'RRR'];
  const black = ['KKK', 'KKK'];
  const base = { width: 3, height: 2 };
  const staticScene = { ...base, frames: [frame(['RGB', 'BGR'], { delayCs: 0 })] };
  const basic = add('static87', { ...staticScene, version: 'GIF87a' }, [[['RGB', 'BGR'], 0]]);
  add('static89', staticScene, [[['RGB', 'BGR'], 0]]);
  add(
    'local-only',
    {
      ...staticScene,
      global: [],
      frames: [frame(['RGB', 'BGR'], { delayCs: 0, palette: ['K', 'B', 'G', 'R'] })],
    },
    [[['RGB', 'BGR'], 0]],
  );
  add(
    'gray-two-colors',
    { width: 3, height: 1, global: ['K', 'W'], frames: [frame(['KWK'], { delayCs: 0 })] },
    [[['KWK'], 0]],
  );
  add(
    'transparent-overlay',
    {
      ...base,
      frames: [
        frame(black),
        frame(['R.R', '.G.'], { transparent: true }),
        frame(['B'], { left: 2, top: 1 }),
      ],
    },
    [
      [black, 5],
      [['RKR', 'KGK'], 5],
      [['RKR', 'KGB'], 5],
    ],
  );
  const partial = {
    ...base,
    frames: [frame(red), frame(['G'], { left: 1 }), frame(['B'], { left: 2, top: 1 })],
  };
  add('offset-keep', partial, [
    [red, 5],
    [['RGR', 'RRR'], 5],
    [['RGR', 'RRB'], 5],
  ]);
  add(
    'local-palette-loop',
    {
      ...base,
      loop: 0,
      frames: [frame(red), frame(['B'], { left: 1, palette: ['K', 'B', 'R', 'G'] })],
    },
    [
      [red, 5],
      [['RBR', 'RRR'], 5],
    ],
  );
  const background = {
    ...base,
    frames: [frame(red), frame(['G'], { left: 1, dispose: 2 }), frame(['B'], { left: 2, top: 1 })],
  };
  add(
    'dispose-background',
    background,
    [
      [red, 5],
      [['RGR', 'RRR'], 5],
      [['R.R', 'RRB'], 5],
    ],
    [
      [red, 5],
      [['RGR', 'RRR'], 5],
      [['RKR', 'RRB'], 5],
    ],
  );
  add(
    'dispose-previous',
    {
      ...base,
      frames: [frame(red), frame(['GB'], { left: 1, dispose: 3 }), frame(['G'], { top: 1 })],
    },
    [
      [red, 5],
      [['RGB', 'RRR'], 5],
      [['RRR', 'GRR'], 5],
    ],
  );
  add(
    'first-previous',
    { ...base, frames: [frame(red, { dispose: 3 }), frame(['B'])] },
    [
      [red, 5],
      [['B..', '...'], 5],
    ],
    [
      [red, 5],
      [['BKK', 'KKK'], 5],
    ],
  );
  add(
    'initial-offset-background',
    { ...base, background: 3, frames: [frame(['R'], { left: 1 })] },
    [[['.R.', '...'], 5]],
    [[['BRB', 'BBB'], 5]],
  );
  const interlaceRows = ['RGB', 'BGR', 'GRB', 'RBG', 'GBR', 'BRG', 'RGB'];
  add(
    'interlaced',
    { width: 3, height: 7, frames: [frame(interlaceRows, { interlaced: true, delayCs: 0 })] },
    [[interlaceRows, 0]],
  );
  // 本项目交错微图的固定无损编码，含跨行字典码；防止严格EOI逐行接口误报回归。
  const compressedInterlace = Buffer.from(
    '47494638376103000700f102000000ff00ff00ff00000000002c00000000030007004002085470266080b25228003b',
    'hex',
  );
  files.set('interlaced-dictionary.gif', compressedInterlace);
  entries.push({
    file: 'interlaced-dictionary.gif',
    sha256: hash(compressedInterlace),
    expect: 'ok',
    expected: entries.find((entry) => entry.file === 'interlaced.gif').expected,
  });
  const unequal = {
    ...base,
    frames: [frame(red, { delayCs: 2 }), frame(['GGG', 'GGG'], { delayCs: 19 })],
    comment: [80, 105, 120, 111, 70, 111, 108, 100, 0, 255],
  };
  add('unequal-delay-comment', unequal, [
    [red, 2],
    [['GGG', 'GGG'], 19],
  ]);
  for (const [name, loop] of [
    ['no-loop', undefined],
    ['finite-loop1', 1],
    ['finite-loop3', 3],
    ['infinite-loop', 0],
  ]) {
    add(name, { ...base, loop, frames: [frame(red), frame(['BBB', 'BBB'])] }, [
      [red, 5],
      [['BBB', 'BBB'], 5],
    ]);
  }
  add('duplicate-visible', { ...base, loop: 0, frames: [frame(red), frame(red, { delayCs: 7 })] }, [
    [red, 5],
    [red, 7],
  ]);
  add('duplicate-merged', { ...base, loop: 0, frames: [frame(red, { delayCs: 12 })] }, [[red, 12]]);
  add(
    'empty-positive-delay',
    {
      ...base,
      loop: 0,
      frames: [frame(red), frame(['.'], { transparent: true, delayCs: 7 }), frame(['BBB', 'BBB'])],
    },
    [
      [red, 5],
      [red, 7],
      [['BBB', 'BBB'], 5],
    ],
  );
  const loopSeam = {
    ...base,
    loop: 0,
    frames: [frame(['R'], { dispose: 1 }), frame(['G'], { left: 2, top: 1 })],
  };
  add(
    'loop-seam-partial',
    loopSeam,
    [
      [['R..', '...'], 5],
      [['R..', '..G'], 5],
    ],
    [
      [['RKK', 'KKK'], 5],
      [['RKK', 'KKG'], 5],
    ],
    {
      // 第二轮保留第一轮末尾的绿像素；两帧归一化为同一画面持续10厘秒。
      secondCycle: {
        transparent: [[['R..', '..G'], 10]],
        logical: [[['RKK', 'KKG'], 10]],
      },
    },
  );
  const largerRows = Array.from({ length: 128 }, (_, y) =>
    Array.from({ length: 256 }, (_, x) => ['R', 'G', 'B', 'K'][((x >> 4) + (y >> 4)) & 3]).join(''),
  );
  const largerReverse = largerRows.toReversed();
  add(
    'larger-pattern',
    {
      width: 256,
      height: 128,
      loop: 0,
      frames: [
        frame(largerRows, { delayCs: 7 }),
        frame(largerReverse, { delayCs: 11 }),
        frame(largerRows, { delayCs: 5 }),
      ],
    },
    [
      [largerRows, 7],
      [largerReverse, 11],
      [largerRows, 5],
    ],
  );
  add('changed-delay', { ...base, frames: [frame(red, { delayCs: 6 }), frame(['BBB', 'BBB'])] }, [
    [red, 6],
    [['BBB', 'BBB'], 5],
  ]);
  add(
    'changed-offset',
    { ...base, frames: [frame(red), frame(['G'], { left: 0 }), frame(['B'], { left: 2, top: 1 })] },
    [
      [red, 5],
      [['GRR', 'RRR'], 5],
      [['GRR', 'RRB'], 5],
    ],
  );
  add('changed-pixel', { ...base, frames: [frame(red), frame(['BBG', 'BBB'])] }, [
    [red, 5],
    [['BBG', 'BBB'], 5],
  ]);
  add('changed-comment', { ...unequal, comment: [80, 105, 120, 111, 70, 111, 108, 100, 0, 254] }, [
    [red, 2],
    [['GGG', 'GGG'], 19],
  ]);
  comparisons.push(
    ...[
      ['duplicate-visible', 'duplicate-merged', true],
      ['no-loop', 'changed-delay', false],
      ['no-loop', 'finite-loop1', false],
      ['finite-loop1', 'finite-loop3', false],
      ['no-loop', 'infinite-loop', false],
      ['offset-keep', 'changed-offset', false],
      ['no-loop', 'changed-pixel', false],
      ['unequal-delay-comment', 'changed-comment', false],
    ].map(([left, right, equivalent]) => ({
      left: left + '.gif',
      right: right + '.gif',
      equivalent,
    })),
  );
  reject('truncated', basic.subarray(0, basic.length - 2));
  reject('trailing-data', Buffer.concat([basic, Buffer.from([0])]));
  reject(
    'zero-screen',
    Buffer.concat([basic.subarray(0, 6), Buffer.from([0, 0]), basic.subarray(8)]),
  );
  reject('outside-frame', encode({ ...base, frames: [frame(['RR'], { left: 2 })] }));
  reject('missing-palette', encode({ ...staticScene, global: [], frames: [frame(['.'])] }));
  reject('reserved-dispose', encode({ ...base, frames: [frame(red, { dispose: 4 })] }));
  reject(
    'user-input',
    encode({ ...base, frames: [frame(red, { userInput: true })] }),
    'UnsupportedInteraction',
  );
  reject(
    'zero-delay-animation',
    encode({ ...base, frames: [frame(red, { delayCs: 0 }), frame(['BBB', 'BBB'])] }),
    'UnsupportedTiming',
  );
  reject(
    'unknown-application',
    encode({ ...staticScene, application: 'PRIVATE0001' }),
    'UnsupportedMetadata',
  );
  const badIndex = Buffer.from(
    encode({ width: 1, height: 1, global: ['K', 'W'], frames: [frame(['K'], { delayCs: 0 })] }),
  );
  badIndex[badIndex.length - 3] = lzw([3], 2)[1];
  badIndex[badIndex.length - 4] = lzw([3], 2)[0];
  reject('invalid-palette-index', badIndex, 'Decode');
  const noEnd = Buffer.from(basic);
  const image = noEnd.indexOf(0x2c, 13);
  noEnd[image + 12] = 0;
  noEnd[image + 13] = 0;
  reject('bad-lzw', noEnd, 'Decode');
  reject(
    'missing-lzw-end',
    encode({ ...staticScene, frames: [frame(['RGB', 'BGR'], { delayCs: 0, omitEnd: true })] }),
    'Decode',
  );
  const many = encode({ ...base, frames: [frame(red), frame(red)] });
  reject('frame-budget', many, 'ResourceLimit', { maxFrames: 1 });
  reject('canvas-budget', basic, 'ResourceLimit', { maxCanvasPixels: 2 });
  reject('frame-pixel-budget', basic, 'ResourceLimit', { maxFramePixels: 2 });
  reject('decoded-budget', basic, 'ResourceLimit', { maxTotalDecodedBytes: 1 });
  reject('input-budget', basic, 'ResourceLimit', { maxInputBytes: 10 });
  reject('metadata-budget', encode(unequal), 'ResourceLimit', { maxMetadataBytes: 2 });
  const manifest = {
    schema: 1,
    origin: 'PixoFold deterministic generated GIFs; GPL-3.0-or-later',
    generator: 'tools/gif-lab/fixtures.mjs',
    samples: entries,
    comparisons,
  };
  files.set(
    'manifest.json',
    Buffer.from(JSON.stringify(manifest, null, 2) + String.fromCharCode(10)),
  );
  return files;
}

export function writeFixtures(directory, check = false) {
  const files = createFixtures();
  if (!check) mkdirSync(directory, { recursive: true });
  for (const [name, bytes] of files) {
    if (check)
      assert.deepEqual(readFileSync(path.join(directory, name)), bytes, '语料/预期未同步：' + name);
    else writeFileSync(path.join(directory, name), bytes);
  }
  const actual = readdirSync(directory)
    .filter((name) => name.endsWith('.gif') || name === 'manifest.json')
    .sort();
  assert.deepEqual(actual, [...files.keys()].sort(), '语料存在未声明文件');
  return files.size - 1;
}
export function stressFixture() {
  const rows = Array.from({ length: 256 }, (_, y) =>
    Array.from({ length: 512 }, (_, x) => ['R', 'G', 'B', 'K'][((x >> 4) + (y >> 4)) & 3]).join(''),
  );
  return encode({
    width: 512,
    height: 256,
    loop: 0,
    frames: Array.from({ length: 16 }, (_, n) =>
      frame(n % 2 ? rows.toReversed() : rows, { delayCs: 5 + n }),
    ),
  });
}
// 单帧2MP用于真实核心的大画布验收，不进入固定45项清单。
export function largeFixture() {
  const rows = Array.from({ length: 1024 }, (_, y) =>
    Array.from({ length: 2048 }, (_, x) => ['R', 'G', 'B', 'K'][((x >> 5) + (y >> 5)) & 3]).join(
      '',
    ),
  );
  return encode({ width: 2048, height: 1024, frames: [frame(rows)] });
}
const ownFile = fileURLToPath(import.meta.url);
if (process.argv[1] && path.resolve(process.argv[1]) === ownFile) {
  const args = process.argv.slice(2);
  assert.ok(args.length === 0 || (args.length === 1 && args[0] === '--check'), '参数：可选--check');
  const count = writeFixtures(
    path.resolve(path.dirname(ownFile), '../../tests/fixtures/gif'),
    args[0] === '--check',
  );
  console.log('GIF确定性语料已核对：' + count + '项');
}
