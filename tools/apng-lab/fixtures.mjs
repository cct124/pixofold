// 独立APNG/存储DEFLATE生成器；小图画布手工填写，不调用Rust或oxipng生成预期。
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
export const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const digest = (b) => createHash('sha256').update(b).digest('hex');
const u32 = (n) => {
  const b = Buffer.alloc(4);
  b.writeUInt32BE(n);
  return b;
};
const u16 = (n) => {
  const b = Buffer.alloc(2);
  b.writeUInt16BE(n);
  return b;
};
function crc32(bytes) {
  let c = 0xffffffff;
  for (const b of bytes) {
    c ^= b;
    for (let i = 0; i < 8; i++) c = (c >>> 1) ^ (c & 1 ? 0xedb88320 : 0);
  }
  return (c ^ 0xffffffff) >>> 0;
}
export function chunk(kind, data = Buffer.alloc(0)) {
  const body = Buffer.concat([Buffer.from(kind), data]);
  return Buffer.concat([u32(data.length), body, u32(crc32(body))]);
}
function stored(bytes) {
  const blocks = [Buffer.from([0x78, 0x01])];
  for (let at = 0; at < bytes.length; at += 65535) {
    const part = bytes.subarray(at, at + 65535);
    const n = part.length;
    blocks.push(
      Buffer.from([at + n === bytes.length ? 1 : 0, n & 255, n >>> 8, ~n & 255, (~n >>> 8) & 255]),
      part,
    );
  }
  let a = 1,
    b = 0;
  for (const v of bytes) {
    a = (a + v) % 65521;
    b = (b + a) % 65521;
  }
  blocks.push(u32((b * 65536 + a) >>> 0));
  return Buffer.concat(blocks);
}
const passes = [
  [0, 0, 8, 8],
  [4, 0, 8, 8],
  [0, 4, 4, 8],
  [2, 0, 4, 4],
  [0, 2, 2, 4],
  [1, 0, 2, 2],
  [0, 1, 1, 2],
];
function raw(frame, depth, interlace) {
  const output = [];
  for (const [x0, y0, dx, dy] of interlace ? passes : [[0, 0, 1, 1]]) {
    if (x0 >= frame.w || y0 >= frame.h) continue;
    for (let y = y0; y < frame.h; y += dy) {
      output.push(Buffer.from([0]));
      for (let x = x0; x < frame.w; x += dx) {
        const pixel = frame.pixels[y * frame.w + x];
        output.push(depth === 16 ? Buffer.concat(pixel.map(u16)) : Buffer.from(pixel));
      }
    }
  }
  return Buffer.concat(output);
}
const R = [255, 0, 0, 255],
  G = [0, 255, 0, 255],
  B = [0, 0, 255, 255],
  W = [255, 255, 255, 255],
  Z = [0, 0, 0, 0];
const rgba = (pixels, extra = {}) => ({
  w: pixels.length,
  h: 1,
  pixels,
  x: 0,
  y: 0,
  num: 1,
  den: 10,
  dispose: 0,
  blend: 0,
  ...extra,
});
export function encode(scene) {
  const {
    w = 2,
    h = 1,
    depth = 8,
    color = 6,
    interlace = 0,
    plays = 0,
    frames,
    poster,
    metadata = [],
    split = false,
  } = scene;
  const chunks = [
    chunk('IHDR', Buffer.concat([u32(w), u32(h), Buffer.from([depth, color, 0, 0, interlace])])),
    ...metadata.map(([k, d]) => chunk(k, d)),
    chunk('acTL', Buffer.concat([u32(frames.length), u32(plays)])),
  ];
  let seq = 0;
  const image = (frame, kind) => {
    const data = stored(raw(frame, depth, interlace));
    for (let at = 0; at < data.length; at += split ? 7 : data.length) {
      const part = data.subarray(at, at + (split ? 7 : data.length));
      chunks.push(chunk(kind, kind === 'fdAT' ? Buffer.concat([u32(seq++), part]) : part));
    }
  };
  if (poster) image(poster, 'IDAT');
  frames.forEach((f, i) => {
    chunks.push(
      chunk(
        'fcTL',
        Buffer.concat([
          u32(seq++),
          u32(f.w),
          u32(f.h),
          u32(f.x ?? 0),
          u32(f.y ?? 0),
          u16(f.num ?? 1),
          u16(f.den ?? 10),
          Buffer.from([f.dispose ?? 0, f.blend ?? 0]),
        ]),
      ),
    );
    image(f, i === 0 && !poster ? 'IDAT' : 'fdAT');
  });
  chunks.push(chunk('IEND'));
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), ...chunks]);
}
function pieces(bytes) {
  const parts = [];
  for (let at = 8; at < bytes.length;) {
    const n = bytes.readUInt32BE(at);
    parts.push([
      bytes.toString('ascii', at + 4, at + 8),
      Buffer.from(bytes.subarray(at + 8, at + 8 + n)),
    ]);
    at += n + 12;
  }
  return parts;
}
function change(bytes, fn) {
  const parts = pieces(bytes);
  fn(parts);
  return Buffer.concat([bytes.subarray(0, 8), ...parts.map(([k, b]) => chunk(k, b))]);
}
export function createFixtures() {
  const files = new Map(),
    samples = [],
    comparisons = [];
  const put = (file, bytes, extra) => {
    files.set(file, bytes);
    samples.push({ file, sha256: digest(bytes), ...extra });
    return bytes;
  };
  const good = (name, scene, canvases) => {
    const rawHash = (frame) =>
      digest(
        Buffer.concat(
          frame.pixels.map((pixel) =>
            scene.depth === 16 ? Buffer.concat(pixel.map(u16)) : Buffer.from(pixel),
          ),
        ),
      );
    return put(name, encode(scene), {
      expect: 'ok',
      defaultRawSha256: rawHash(scene.poster ?? scene.frames[0]),
      frameRawSha256: scene.frames.map(rawHash),
      structure: {
        width: scene.w ?? 2,
        height: scene.h ?? 1,
        depth: scene.depth ?? 8,
        color: scene.color ?? 6,
        interlace: scene.interlace ?? 0,
        plays: scene.plays ?? 0,
        defaultParticipates: !scene.poster,
      },
      controls: scene.frames.map((f) => ({
        width: f.w,
        height: f.h,
        x: f.x ?? 0,
        y: f.y ?? 0,
        delayNum: f.num ?? 1,
        delayDen: f.den ?? 10,
        dispose: f.dispose ?? 0,
        blend: f.blend ?? 0,
      })),
      canvases: canvases.map((frame) => frame.map((pixel) => pixel.map((n) => n * 257))),
    });
  };
  const base = { frames: [rgba([R, R]), rgba([B], { x: 1 })] };
  const basic = good('default-in.png', base, [
    [R, R],
    [R, B],
  ]);
  put('extension.APNG', basic, { ...samples[0], file: 'extension.APNG' });
  put('extension.PNG', basic, { ...samples[0], file: 'extension.PNG' });
  good('default-out.apng', { poster: rgba([W, W]), frames: [rgba([B], { x: 1 }), rgba([R])] }, [
    [Z, B],
    [R, B],
  ]);
  good('over.apng', { frames: [rgba([R, R]), rgba([[0, 0, 255, 128]], { x: 1, blend: 1 })] }, [
    [R, R],
    [R, [127, 0, 128, 255]],
  ]);
  good(
    'background.apng',
    { frames: [rgba([R, R]), rgba([B], { dispose: 1 }), rgba([G], { x: 1 })] },
    [
      [R, R],
      [B, R],
      [Z, G],
    ],
  );
  good(
    'previous.apng',
    { frames: [rgba([R, R]), rgba([B], { dispose: 2 }), rgba([G], { x: 1 })] },
    [
      [R, R],
      [B, R],
      [R, G],
    ],
  );
  good('first-previous.apng', { frames: [rgba([R, R], { dispose: 2 }), rgba([B], { x: 1 })] }, [
    [R, R],
    [Z, B],
  ]);
  good(
    'first-over-transparent.apng',
    { poster: rgba([W, W]), frames: [rgba([[255, 0, 0, 128]], { x: 1, blend: 1 }), rgba([B])] },
    [
      [Z, [255, 0, 0, 128]],
      [B, [255, 0, 0, 128]],
    ],
  );
  good(
    'hidden-rgb.apng',
    { frames: [rgba([R, R]), rgba([[17, 33, 55, 0]]), rgba([[0, 0, 0, 0]], { blend: 1 })] },
    [
      [R, R],
      [[17, 33, 55, 0], R],
      [[17, 33, 55, 0], R],
    ],
  );
  good(
    'zero-and-denominator.apng',
    {
      plays: 2,
      frames: [
        rgba([R, R], { num: 0, den: 0 }),
        rgba([B], { x: 1, num: 1, den: 3 }),
        rgba([G], { num: 65535, den: 65535 }),
      ],
    },
    [
      [R, R],
      [R, B],
      [G, B],
    ],
  );
  good('one-play.apng', { ...base, plays: 1 }, [
    [R, R],
    [R, B],
  ]);
  good('split-data.apng', { ...base, split: true }, [
    [R, R],
    [R, B],
  ]);
  good('gray8.apng', { color: 0, frames: [rgba([[17], [255]])] }, [[[17, 17, 17, 255], W]]);
  good(
    'gray-alpha8.apng',
    {
      color: 4,
      frames: [
        rgba([
          [17, 0],
          [255, 128],
        ]),
      ],
    },
    [
      [
        [17, 17, 17, 0],
        [255, 255, 255, 128],
      ],
    ],
  );
  good(
    'rgb8.apng',
    {
      color: 2,
      frames: [
        rgba([
          [255, 0, 0],
          [0, 0, 255],
        ]),
      ],
    },
    [[R, B]],
  );
  good(
    'rgb-trns.apng',
    {
      color: 2,
      metadata: [['tRNS', Buffer.concat([u16(255), u16(0), u16(0)])]],
      frames: [
        rgba([
          [255, 0, 0],
          [0, 0, 255],
        ]),
      ],
    },
    [[[255, 0, 0, 0], B]],
  );
  good(
    'gray-trns.apng',
    { color: 0, metadata: [['tRNS', u16(17)]], frames: [rgba([[17], [255]])] },
    [[[17, 17, 17, 0], W]],
  );
  good(
    'palette.apng',
    {
      color: 3,
      metadata: [
        ['PLTE', Buffer.from([255, 0, 0, 0, 0, 255])],
        ['tRNS', Buffer.from([0, 128])],
      ],
      frames: [rgba([[0], [1]])],
    },
    [
      [
        [255, 0, 0, 0],
        [0, 0, 255, 128],
      ],
    ],
  );
  for (const [name, color, pixels, expected] of [
    [
      'gray16',
      0,
      [[0x1234], [65535]],
      [
        [0x1234, 0x1234, 0x1234, 65535],
        [65535, 65535, 65535, 65535],
      ],
    ],
    [
      'rgb16',
      2,
      [
        [0x1234, 0x2345, 0x3456],
        [65535, 0, 0],
      ],
      [
        [0x1234, 0x2345, 0x3456, 65535],
        [65535, 0, 0, 65535],
      ],
    ],
    [
      'gray-alpha16',
      4,
      [
        [0x1234, 0],
        [65535, 0x4567],
      ],
      [
        [0x1234, 0x1234, 0x1234, 0],
        [65535, 65535, 65535, 0x4567],
      ],
    ],
    [
      'rgba16',
      6,
      [
        [0x1234, 0x2345, 0x3456, 0],
        [0, 0, 65535, 0x4567],
      ],
      [
        [0x1234, 0x2345, 0x3456, 0],
        [0, 0, 65535, 0x4567],
      ],
    ],
  ]) {
    good(name + '.apng', { depth: 16, color, frames: [rgba(pixels)] }, [[R, R]]);
    samples.at(-1).canvases = [expected];
  }
  const grid = [R, G, B, W, R, G, B, W, R, G, B, W, R, G, B, W];
  good(
    'adam7.apng',
    { w: 4, h: 4, interlace: 1, frames: [rgba(grid, { w: 4, h: 4 }), rgba([B], { x: 1, y: 2 })] },
    [grid, grid.map((p, i) => (i === 9 ? B : p))],
  );
  const metadata = [
    ['gAMA', u32(45455)],
    ['sRGB', Buffer.from([0])],
    ['pHYs', Buffer.concat([u32(3780), u32(3780), Buffer.from([1])])],
    ['tEXt', Buffer.from('Author\0PixoFold synthetic')],
  ];
  good('metadata.apng', { ...base, metadata }, [
    [R, R],
    [R, B],
  ]);
  good('changed-pixel.apng', { frames: [rgba([R, R]), rgba([G], { x: 1 })] }, [
    [R, R],
    [R, G],
  ]);
  good('changed-delay.apng', { frames: [rgba([R, R]), rgba([B], { x: 1, num: 2 })] }, [
    [R, R],
    [R, B],
  ]);
  good('changed-offset.apng', { frames: [rgba([R, R]), rgba([B])] }, [
    [R, R],
    [B, R],
  ]);
  good('changed-loop.apng', { ...base, plays: 3 }, [
    [R, R],
    [R, B],
  ]);
  good('changed-poster.apng', { poster: rgba([R, R]), frames: [rgba([B], { x: 1 }), rgba([R])] }, [
    [Z, B],
    [R, B],
  ]);
  good(
    'changed-metadata.apng',
    { ...base, metadata: [...metadata.slice(0, -1), ['tEXt', Buffer.from('Author\0Changed')]] },
    [
      [R, R],
      [R, B],
    ],
  );
  for (const other of ['changed-pixel', 'changed-delay', 'changed-offset', 'changed-loop'])
    comparisons.push({ a: 'default-in.png', b: other + '.apng', equivalent: false });
  comparisons.push(
    { a: 'default-in.png', b: 'split-data.apng', equivalent: true },
    { a: 'default-out.apng', b: 'changed-poster.apng', equivalent: false },
    { a: 'metadata.apng', b: 'changed-metadata.apng', equivalent: false },
  );
  const bad = (name, bytes, expect, limits) =>
    put(name + '.apng', bytes, { expect, ...(limits ? { limits } : {}) });
  const mutate = (name, fn, expect) => bad(name, change(basic, fn), expect);
  good(
    'denominator-zero.apng',
    { frames: [rgba([R, R], { num: 1, den: 0 }), rgba([B], { x: 1 })] },
    [
      [R, R],
      [R, B],
    ],
  );
  good(
    'denominator-hundred.apng',
    { frames: [rgba([R, R], { num: 1, den: 100 }), rgba([B], { x: 1 })] },
    [
      [R, R],
      [R, B],
    ],
  );
  comparisons.push({
    a: 'denominator-zero.apng',
    b: 'denominator-hundred.apng',
    equivalent: false,
  });
  good('changed-dispose.apng', { frames: [rgba([R, R], { dispose: 1 }), rgba([B], { x: 1 })] }, [
    [R, R],
    [Z, B],
  ]);
  comparisons.push({ a: 'default-in.png', b: 'changed-dispose.apng', equivalent: false });
  good('changed-blend.apng', { frames: [rgba([R, R]), rgba([[0, 0, 255, 128]], { x: 1 })] }, [
    [R, R],
    [R, [0, 0, 255, 128]],
  ]);
  comparisons.push({ a: 'over.apng', b: 'changed-blend.apng', equivalent: false });
  good(
    'half-over-half.apng',
    { frames: [rgba([[255, 0, 0, 128], R]), rgba([[0, 0, 255, 128]], { blend: 1 })] },
    [
      [[255, 0, 0, 128], R],
      [R, R],
    ],
  );
  samples.at(-1).canvases[1] = [[21788, 0, 43747, 49279], R.map((n) => n * 257)];
  good(
    'rgba16-over.apng',
    {
      depth: 16,
      frames: [
        rgba([
          [65535, 0, 0, 65535],
          [65535, 0, 0, 65535],
        ]),
        rgba([[0, 0, 65535, 32768]], { x: 1, blend: 1 }),
      ],
    },
    [
      [R, R],
      [R, R],
    ],
  );
  samples.at(-1).canvases[1][1] = [32767, 0, 32768, 65535];
  good(
    'changed-hidden.apng',
    { frames: [rgba([R, R]), rgba([[18, 33, 55, 0]]), rgba([[0, 0, 0, 0]], { blend: 1 })] },
    [
      [R, R],
      [[18, 33, 55, 0], R],
      [[18, 33, 55, 0], R],
    ],
  );
  comparisons.push({ a: 'hidden-rgb.apng', b: 'changed-hidden.apng', equivalent: false });
  bad(
    'palette-index',
    encode({
      color: 3,
      metadata: [['PLTE', Buffer.from([255, 0, 0, 0, 0, 255])]],
      frames: [rgba([[0], [2]])],
    }),
    'decode',
  );
  mutate(
    'idat-gap',
    (p) => {
      const i = p.findIndex(([k]) => k === 'IDAT');
      const b = p[i][1];
      p.splice(
        i,
        1,
        ['IDAT', b.subarray(0, 5)],
        ['tEXt', Buffer.from('Key\0value')],
        ['IDAT', b.subarray(5)],
      );
    },
    'idat_order',
  );
  mutate(
    'fdat-before-control',
    (p) => {
      const f = p.splice(
        p.findIndex(([k]) => k === 'fdAT'),
        1,
      );
      p.splice(2, 0, ...f);
    },
    'fdat_order',
  );
  bad('truncated', basic.subarray(0, -1), 'truncated_or_trailing');
  bad('trailing', Buffer.concat([basic, Buffer.from([0])]), 'truncated_or_trailing');
  const crc = Buffer.from(basic);
  crc[32] ^= 1;
  bad('bad-crc', crc, 'crc');
  bad('fake-extension', Buffer.from('not a png'), 'signature');
  mutate('missing-actl', (p) => p.splice(1, 1), 'fctl_order');
  mutate(
    'late-actl',
    (p) => {
      const a = p.splice(1, 1);
      p.splice(3, 0, ...a);
    },
    'fctl_order',
  );
  mutate('duplicate-actl', (p) => p.splice(2, 0, p[1]), 'actl_order');
  mutate('bad-sequence', (p) => p.find(([k]) => k === 'fdAT')[1].writeUInt32BE(9), 'sequence');
  mutate(
    'duplicate-sequence',
    (p) => p.filter(([k]) => k === 'fcTL')[1][1].writeUInt32BE(0),
    'sequence',
  );
  mutate('wrong-count', (p) => p[1][1].writeUInt32BE(3), 'frame_count');
  mutate('zero-count', (p) => p[1][1].writeUInt32BE(0), 'frame_count');
  mutate('zero-canvas', (p) => p[0][1].writeUInt32BE(0), 'dimensions');
  mutate('zero-frame', (p) => p[2][1].writeUInt32BE(0, 4), 'frame_bounds');
  mutate(
    'outside',
    (p) => p.filter(([k]) => k === 'fcTL')[1][1].writeUInt32BE(2, 12),
    'frame_bounds',
  );
  mutate(
    'first-offset',
    (p) => {
      p[2][1].writeUInt32BE(1, 4);
      p[2][1].writeUInt32BE(1, 12);
    },
    'default_frame_bounds',
  );
  mutate(
    'bad-dispose',
    (p) => {
      p[2][1][24] = 3;
    },
    'frame_operation',
  );
  mutate(
    'bad-blend',
    (p) => {
      p[2][1][25] = 2;
    },
    'frame_operation',
  );
  mutate(
    'missing-frame-data',
    (p) =>
      p.splice(
        p.findIndex(([k]) => k === 'fdAT'),
        1,
      ),
    'missing_frame_data',
  );
  mutate(
    'bad-deflate',
    (p) => {
      p.find(([k]) => k === 'fdAT')[1][4] = 0;
    },
    'decode',
  );
  mutate(
    'missing-adler',
    (p) => {
      const f = p.find(([k]) => k === 'fdAT');
      f[1] = f[1].subarray(0, -4);
    },
    'decode',
  );
  mutate(
    'bad-adler',
    (p) => {
      const f = p.find(([k]) => k === 'fdAT');
      f[1][f[1].length - 1] ^= 1;
    },
    'decode',
  );
  mutate(
    'zlib-trailing',
    (p) => {
      const f = p.find(([k]) => k === 'fdAT');
      f[1] = Buffer.concat([f[1], Buffer.from([0])]);
    },
    'decode',
  );
  mutate(
    'extra-decoded',
    (p) => {
      const f = p.find(([k]) => k === 'fdAT');
      f[1] = Buffer.concat([f[1].subarray(0, 4), stored(Buffer.alloc(6))]);
    },
    'decode',
  );
  mutate('unknown-critical', (p) => p.splice(1, 0, ['ABCD', Buffer.alloc(0)]), 'unknown_critical');
  for (const name of ['caBX', 'iCCP', 'eXIf', 'vpAg', 'zTXt', 'sBIT', 'bKGD'])
    mutate(
      'metadata-' + name,
      (p) => p.splice(1, 0, [name, Buffer.from('synthetic')]),
      'unsupported_metadata',
    );
  mutate(
    'low-depth',
    (p) => {
      p[0][1][8] = 4;
      p[0][1][9] = 3;
    },
    'unsupported_representation',
  );
  for (const [name, key, n, code] of [
    ['input', 'maxInputBytes', 8, 'input_budget'],
    ['canvas', 'maxCanvasPixels', 1, 'canvas_budget'],
    ['frame-pixel', 'maxFramePixels', 1, 'frame_pixel_budget'],
    ['total-pixel', 'maxTotalPixels', 2, 'total_pixel_budget'],
    ['decoded', 'maxDecodedBytes', 1, 'decoded_budget'],
    ['frames', 'maxFrames', 1, 'frame_budget'],
    ['chunks', 'maxChunks', 2, 'chunk_budget'],
    ['validation', 'maxValidationBytes', 10, 'validation_budget'],
    ['deadline', 'deadlineMs', 0, 'deadline'],
  ])
    bad('budget-' + name, basic, code, { [key]: n });
  bad('budget-metadata', files.get('metadata.apng'), 'metadata_budget', { maxMetadataBytes: 1 });
  const manifest = {
    schema: 1,
    origin: 'PixoFold generated APNG; GPL-3.0-or-later',
    generator: 'tools/apng-lab/fixtures.mjs',
    canvasEncoding: 'RGBA16 integers; 8-bit channels multiply 257; row major; manual expectations',
    samples,
    comparisons,
  };
  files.set('manifest.json', Buffer.from(JSON.stringify(manifest, null, 2) + '\n'));
  return files;
}
export function writeFixtures(check = false) {
  const directory = path.join(root, 'tests/fixtures/apng'),
    files = createFixtures();
  if (!check) mkdirSync(directory, { recursive: true });
  for (const [file, bytes] of files) {
    if (check) assert.deepEqual(readFileSync(path.join(directory, file)), bytes, file);
    else writeFileSync(path.join(directory, file), bytes);
  }
  assert.deepEqual(
    readdirSync(directory)
      .filter((n) => n !== 'README.md')
      .sort(),
    [...files.keys()].sort(),
  );
  return files.size - 1;
}
export function stressFixture() {
  const w = 512,
    h = 256;
  const pixels = Array.from({ length: w * h }, (_, i) =>
    (((i % w) >> 4) + (Math.floor(i / w) >> 4)) % 2 ? R : B,
  );
  return encode({
    w,
    h,
    frames: Array.from({ length: 12 }, (_, n) =>
      rgba(n % 2 ? pixels.toReversed() : pixels, { w, h }),
    ),
  });
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  assert.ok(
    process.argv.length === 2 || (process.argv.length === 3 && process.argv[2] === '--check'),
  );
  console.log('APNG确定性语料：' + writeFixtures(process.argv[2] === '--check'));
}
