// PixoFold 自生成语料；无外部图片。固定 Node 版本，--check 只读检查字节和清单。
import { deflateSync } from 'node:zlib';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const directory = new URL('./png/', import.meta.url);
const check = process.argv.includes('--check');
const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const u32 = (value) => {
  const b = Buffer.alloc(4);
  b.writeUInt32BE(value);
  return b;
};
const crc32 = (bytes) => {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
};
const chunk = (name, data) => {
  const body = Buffer.concat([Buffer.from(name, 'ascii'), data]);
  return Buffer.concat([u32(data.length), body, u32(crc32(body))]);
};
const hash = (data) => createHash('sha256').update(data).digest('hex');
const entries = [];
const generated = new Map();
const store = (name, data, properties) => {
  generated.set(name, data);
  entries.push({ file: name, bytes: data.length, sha256: hash(data), ...properties });
};
const channels = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 };

function makePng({
  width = 64,
  height = 48,
  depth = 8,
  color = 6,
  adam7 = false,
  before = [],
  after = [],
  level = 0,
} = {}) {
  const samples = channels[color];
  const sample = (x, y, c) => {
    if ((color === 6 && c === 3) || (color === 4 && c === 1))
      return [0, 1, 85, 128, 254, 255][x % 6] * (depth === 16 ? 257 : 1);
    if (color === 3 || depth < 8) return (x + y) % (1 << depth);
    const value = ((x % 8) * 19 + (y % 4) * 23 + c * 47) & 255;
    return depth === 16 ? value * 256 + ((x * 3 + c * 7) & 255) : value;
  };
  const passes = adam7
    ? [
        [0, 0, 8, 8],
        [4, 0, 8, 8],
        [0, 4, 4, 8],
        [2, 0, 4, 4],
        [0, 2, 2, 4],
        [1, 0, 2, 2],
        [0, 1, 1, 2],
      ]
    : [[0, 0, 1, 1]];
  const rows = [];
  for (const [x0, y0, dx, dy] of passes) {
    if (x0 >= width || y0 >= height) continue;
    for (let y = y0; y < height; y += dy) {
      const count = Math.ceil((width - x0) / dx);
      const row = Buffer.alloc(1 + Math.ceil((count * samples * depth) / 8));
      let index = 0;
      for (let x = x0; x < width; x += dx) {
        for (let c = 0; c < samples; c++, index++) {
          const value = sample(x, y, c);
          if (depth === 16) row.writeUInt16BE(value, 1 + index * 2);
          else if (depth === 8) row[1 + index] = value;
          else
            row[1 + Math.floor((index * depth) / 8)] |=
              value << (8 - depth - ((index * depth) % 8));
        }
      }
      rows.push(row);
    }
  }
  const ihdr = Buffer.concat([
    u32(width),
    u32(height),
    Buffer.from([depth, color, 0, 0, adam7 ? 1 : 0]),
  ]);
  const palette = [];
  if (color === 3) {
    const colors = Buffer.alloc(3 * (1 << depth));
    const alpha = Buffer.alloc(1 << depth);
    for (let i = 0; i < alpha.length; i++) {
      colors.set([(i * 31) & 255, (i * 67) & 255, 255 - i], i * 3);
      alpha[i] = i === 0 ? 0 : i === 1 ? 128 : 255;
    }
    palette.push(chunk('PLTE', colors), chunk('tRNS', alpha));
  }
  const compressed = deflateSync(Buffer.concat(rows), { level });
  return Buffer.concat([
    signature,
    chunk('IHDR', ihdr),
    ...before,
    ...palette,
    chunk('IDAT', compressed),
    ...after,
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// 自生成 ICC v4 matrix/shaper RGB profile；本语料仅验证原始 iCCP 保留，不作色彩校准。
function iccProfile() {
  const fixed = (n) => {
    const b = Buffer.alloc(4);
    b.writeInt32BE(Math.round(n * 65536));
    return b;
  };
  const xyz = (x, y, z) =>
    Buffer.concat([Buffer.from('XYZ '), Buffer.alloc(4), fixed(x), fixed(y), fixed(z)]);
  const curve = Buffer.concat([
    Buffer.from('curv'),
    Buffer.alloc(4),
    u32(1),
    Buffer.from([2, 51, 0, 0]),
  ]);
  const tags = [
    ['wtpt', xyz(0.9642, 1, 0.8249)],
    ['rXYZ', xyz(0.4361, 0.2225, 0.0139)],
    ['gXYZ', xyz(0.3851, 0.7169, 0.0971)],
    ['bXYZ', xyz(0.1431, 0.0606, 0.7141)],
    ['rTRC', curve],
    ['gTRC', curve],
    ['bTRC', curve],
  ];
  const header = Buffer.alloc(128);
  header.writeUInt32BE(0x04300000, 8);
  header.write('mntrRGB XYZ ', 12, 'ascii');
  [2026, 9, 22, 0, 0, 0].forEach((value, i) => header.writeUInt16BE(value, 24 + i * 2));
  header.write('acsp', 36, 'ascii');
  header.set(Buffer.concat([fixed(0.9642), fixed(1), fixed(0.8249)]), 68);
  let offset = 132 + tags.length * 12;
  const table = tags.map(([name, value]) => {
    const record = Buffer.concat([Buffer.from(name), u32(offset), u32(value.length)]);
    offset += value.length;
    return record;
  });
  header.writeUInt32BE(offset, 0);
  return Buffer.concat([header, u32(tags.length), ...table, ...tags.map(([, value]) => value)]);
}

for (const [name, color, depth] of [
  ['rgb8', 2, 8],
  ['rgba8', 6, 8],
  ['gray8', 0, 8],
  ['gray16', 0, 16],
  ['gray-alpha8', 4, 8],
  ['gray-alpha16', 4, 16],
  ['rgb16', 2, 16],
  ['rgba16', 6, 16],
  ['indexed1', 3, 1],
  ['indexed2', 3, 2],
  ['indexed4', 3, 4],
  ['indexed8', 3, 8],
  ['gray1', 0, 1],
]) {
  store(name + '.png', makePng({ color, depth }), {
    expected: 'static',
    width: 64,
    height: 48,
    color,
    depth,
  });
}
store('adam7-rgba8.png', makePng({ adam7: true }), {
  expected: 'static',
  width: 64,
  height: 48,
  color: 6,
  depth: 8,
  interlaced: true,
});
const exif = Buffer.from('49492a0008000000010012010300010000000600000000000000', 'hex');
const display = [
  chunk('gAMA', u32(45455)),
  chunk('sRGB', Buffer.from([0])),
  chunk('pHYs', Buffer.concat([u32(3780), u32(3780), Buffer.from([1])])),
  chunk('eXIf', exif),
];
store(
  'display-metadata.png',
  makePng({
    before: display,
    after: [chunk('tEXt', Buffer.from('Source\0PixoFold synthetic fixture'))],
  }),
  { expected: 'static', metadata: ['gAMA', 'sRGB', 'pHYs', 'eXIf', 'tEXt'] },
);
store(
  'icc-rgb8.png',
  makePng({
    color: 2,
    before: [
      chunk(
        'iCCP',
        Buffer.concat([Buffer.from('PixoFold\0\0'), deflateSync(iccProfile(), { level: 9 })]),
      ),
    ],
  }),
  { expected: 'static', metadata: ['iCCP'] },
);
store(
  'trns-rgb8.png',
  makePng({ color: 2, before: [chunk('tRNS', Buffer.from([0, 0, 0, 47, 0, 94]))] }),
  { expected: 'static', metadata: ['tRNS'] },
);
store(
  'trns-gray16.png',
  makePng({ color: 0, depth: 16, before: [chunk('tRNS', Buffer.from([0, 0]))] }),
  { expected: 'static', metadata: ['tRNS'] },
);
store('already-optimized.png', makePng({ width: 1, height: 1, color: 0, level: 9 }), {
  expected: 'no-gain',
  width: 1,
  height: 1,
  depth: 8,
  color: 0,
});
store('real-png.jpg', generated.get('rgb8.png'), {
  expected: 'static',
  note: '扩展名不能决定格式',
});
store('fake.png', Buffer.from('not an image\n'), { expected: 'unsupported-format' });
const rgba = generated.get('rgba8.png');
store('truncated.png', rgba.subarray(0, rgba.length - 5), { expected: 'invalid' });
const corrupt = Buffer.from(rgba);
corrupt[corrupt.length - 1] ^= 1;
store('bad-crc.png', corrupt, { expected: 'invalid' });
store('trailing-data.png', Buffer.concat([rgba, Buffer.from('unexpected')]), {
  expected: 'invalid',
});
store(
  'bad-deflate.png',
  Buffer.concat([
    rgba.subarray(0, 33),
    chunk('IDAT', Buffer.from([0, 1, 2, 3])),
    chunk('IEND', Buffer.alloc(0)),
  ]),
  { expected: 'invalid' },
);
const control = (sequence) =>
  Buffer.concat([
    u32(sequence),
    u32(64),
    u32(48),
    u32(0),
    u32(0),
    Buffer.from([0, 1, 0, 10, 0, 0]),
  ]);
const idatLength = rgba.readUInt32BE(33);
const idat = rgba.subarray(41, 41 + idatLength);
store(
  'animated.png',
  Buffer.concat([
    rgba.subarray(0, 33),
    chunk('acTL', Buffer.concat([u32(2), u32(0)])),
    chunk('fcTL', control(0)),
    chunk('IDAT', idat),
    chunk('fcTL', control(1)),
    chunk('fdAT', Buffer.concat([u32(2), idat])),
    chunk('IEND', Buffer.alloc(0)),
  ]),
  { expected: 'unsupported-animation', frames: 2 },
);
// 动画标记放在默认图像之后，防止仅检查 read_info 的实现丢掉后续帧。
store(
  'animation-after-idat.png',
  Buffer.concat([
    rgba.subarray(0, 33),
    chunk('acTL', Buffer.concat([u32(1), u32(0)])),
    chunk('IDAT', idat),
    chunk('fcTL', control(0)),
    chunk('fdAT', Buffer.concat([u32(1), idat])),
    chunk('IEND', Buffer.alloc(0)),
  ]),
  { expected: 'unsupported-animation', defaultImageSeparate: true },
);

generated.set(
  'manifest.json',
  Buffer.from(
    JSON.stringify(
      {
        source: 'PixoFold 自生成；GPL-3.0-or-later；无私人图片或外部素材',
        generator: 'tests/fixtures/generate.mjs',
        fixtures: entries,
      },
      null,
      2,
    ) + '\n',
  ),
);
if (!check) mkdirSync(directory, { recursive: true });
for (const [name, data] of generated) {
  const path = new URL(name, directory);
  if (check) {
    if (!readFileSync(path).equals(data)) throw new Error('语料不一致：' + fileURLToPath(path));
  } else writeFileSync(path, data);
}
console.log(
  (check ? '已验证' : '已生成') + ' ' + entries.length + ' 个 PNG 边界样本与 SHA256 清单',
);
