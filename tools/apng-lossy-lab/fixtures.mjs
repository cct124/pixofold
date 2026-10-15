// 本项目生成的有损压力语料；保持明文场景配方与输出哈希，不下载额外资源。
import { encode, stressFixture, chunk } from '../apng-lab/fixtures.mjs';
const frame = (w, h, pixel, extra = {}) => ({
  w,
  h,
  pixels: Array.from({ length: w * h }, (_, i) => pixel(i % w, Math.floor(i / w))),
  ...extra,
});
export function lossyFixtures() {
  const gradient = (x, y, n) => [
    (x * 3 + n * 7) % 256,
    (y * 4 + n * 3) % 256,
    (x + y * 2 + n * 11) % 256,
    255,
  ];
  const scenes = [
    [
      'moving-gradient',
      {
        w: 128,
        h: 80,
        frames: Array.from({ length: 6 }, (_, n) =>
          frame(128, 80, (x, y) => gradient(x, y, n), { num: n + 1, den: 60 }),
        ),
      },
    ],
    [
      'alpha-overlays',
      {
        w: 96,
        h: 64,
        frames: [
          frame(96, 64, () => [0, 0, 0, 0], { dispose: 1, blend: 1 }),
          ...Array.from({ length: 6 }, (_, n) =>
            frame(
              64,
              40,
              (x, y) => [...gradient(x, y, n).slice(0, 3), [0, 64, 128, 192, 255][(x + y + n) % 5]],
              { x: n * 3, y: n * 2, blend: 1, dispose: n % 3, num: 7, den: 0 },
            ),
          ),
        ],
      },
    ],
    [
      'changing-colors',
      {
        w: 64,
        h: 48,
        frames: Array.from({ length: 12 }, (_, n) =>
          frame(
            64,
            48,
            (x, y) => [(n * 19 + x * 2) % 256, (n * 29 + y * 3) % 256, (x * y + n * 11) % 256, 255],
            { dispose: n % 3 },
          ),
        ),
      },
    ],
    [
      'independent-poster',
      {
        w: 80,
        h: 48,
        poster: frame(80, 48, (x, y) => gradient(x, y, 4)),
        frames: [
          frame(32, 24, (x, y) => [255, (x + y) % 64, 0, 255], {
            x: 20,
            y: 8,
            blend: 1,
            dispose: 2,
          }),
          frame(32, 24, (x, y) => [0, 255, (x + y) % 64, 128], {
            x: 24,
            y: 12,
            blend: 1,
            dispose: 1,
          }),
        ],
      },
    ],
    [
      'repeated-gradient',
      {
        w: 96,
        h: 64,
        frames: Array.from({ length: 6 }, (_, n) =>
          frame(96, 64, (x, y) => gradient(x, y, Math.floor(n / 2)), {
            num: n % 2 ? 1 : 3,
            den: 60,
          }),
        ),
      },
    ],
  ];
  const samples = scenes.map(([name, scene]) => ({ name, bytes: encode(scene), expect: 'ok' }));
  // 采样不能代表所有alpha：稀有端点、相邻alpha及长序列OVER需单独约束。
  for (const [name, scene] of [
    [
      'alpha-ramp',
      {
        w: 256,
        h: 2,
        frames: [
          frame(256, 2, (x, y) => [220, y ? 80 : 40, 20, x]),
          frame(256, 2, (x) => [20, 80, 220, x], { blend: 1 }),
        ],
      },
    ],
    [
      'rare-alpha',
      {
        w: 128,
        h: 64,
        poster: frame(128, 64, (x, y) =>
          x === 127 && y === 63 ? [255, 0, 255, 1] : [20, 120, 200, 255],
        ),
        frames: [
          frame(128, 64, (x, y) => (x === 1 && y === 1 ? [255, 255, 0, 254] : [20, 120, 200, 255])),
        ],
      },
    ],
    [
      'accumulated-over',
      {
        w: 32,
        h: 16,
        frames: Array.from({ length: 24 }, (_, n) =>
          frame(32, 16, (x, y) => [n % 2 ? 240 : 20, 120, x * 7, ((x + y) % 31) + 1], {
            blend: 1,
            dispose: 0,
            num: 1,
            den: 60,
          }),
        ),
      },
    ],
  ])
    samples.push({ name, bytes: encode(scene), expect: 'ok' });
  samples.push({ name: 'stress', bytes: stressFixture(), expect: 'ok' });
  const base = { w: 2, h: 1, frames: [frame(2, 1, () => [120, 80, 40, 255])] };
  const gamma = Buffer.alloc(4);
  gamma.writeUInt32BE(100000);
  samples.push({
    name: 'linear-gamma',
    bytes: encode({ ...base, metadata: [['gAMA', gamma]] }),
    expect: 'unsupported_color',
  });
  samples.push({
    name: 'chromaticities',
    bytes: encode({ ...base, metadata: [['cHRM', Buffer.alloc(32)]] }),
    expect: 'unsupported_color',
  });
  const mismatched = Buffer.alloc(4);
  mismatched.writeUInt32BE(50000);
  samples.push({
    name: 'srgb-gamma-conflict',
    bytes: encode({
      ...base,
      metadata: [
        ['gAMA', mismatched],
        ['sRGB', Buffer.from([0])],
      ],
    }),
    expect: 'unsupported_color',
  });
  // 元数据位于动画帧之后，重建不能将其移动到头部。
  const trailing = encode(base);
  samples.push({
    name: 'trailing-text',
    bytes: Buffer.concat([
      trailing.subarray(0, -12),
      chunk('tEXt', Buffer.from('Comment\0After frame')),
      trailing.subarray(-12),
    ]),
    expect: 'ok',
  });
  return samples;
}
