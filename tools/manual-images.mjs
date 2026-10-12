/** 校验Git中的公开手测原件，或在.temp独占创建一份可覆盖测试的副本。 */
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { lstat, mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = fileURLToPath(new URL('../', import.meta.url));
const sourceRoot = path.join(repoRoot, 'tests', 'manual-images');
const command = process.argv[2];
assert.ok(
  process.argv.length === 3 && ['check', 'prepare'].includes(command),
  '用法：node tools/manual-images.mjs check|prepare',
);

const manifest = JSON.parse(await readFile(path.join(sourceRoot, 'SOURCES.json'), 'utf8'));
assert.equal(manifest.result, 'passed');
assert.equal(manifest.count, manifest.files.length);
const names = new Set();
const verified = [];
let totalBytes = 0;

// 保留校验过的字节到复制结束，避免随后重新读取变化中的原件。
for (const entry of [...manifest.files, ...manifest.license_documents]) {
  assert.match(entry.file, /^(png|jpeg|gif|apng|licenses)\/[A-Za-z0-9_.-]+$/);
  assert.match(entry.sha256, /^[a-f0-9]{64}$/);
  assert.ok(!names.has(entry.file), '重复路径：' + entry.file);
  names.add(entry.file);
  const file = path.join(sourceRoot, entry.file);
  assert.ok((await lstat(file)).isFile(), '需要普通文件：' + entry.file);
  const bytes = await readFile(file);
  assert.equal(
    createHash('sha256').update(bytes).digest('hex'),
    entry.sha256,
    '原件SHA256不一致：' + entry.file + '；请从Git恢复原件后重试。',
  );
  if (!entry.file.startsWith('licenses/')) {
    assert.equal(bytes.length, entry.bytes, '原件大小不一致：' + entry.file);
    totalBytes += bytes.length;
    verified.push({ file: entry.file, bytes });
  }
}
assert.equal(totalBytes, manifest.total_image_bytes);
const sums = (await readFile(path.join(sourceRoot, 'SHA256SUMS.txt'), 'utf8'))
  .trim()
  .split(/\r?\n/);
assert.deepEqual(
  sums,
  manifest.files.map((entry) => entry.sha256 + '  ' + entry.file),
  'SHA256SUMS.txt与SOURCES.json不一致。',
);
console.log(
  '已校验' +
    verified.length +
    '张原件、' +
    manifest.license_documents.length +
    '份许可，共' +
    totalBytes +
    ' bytes。',
);

if (command === 'prepare') {
  const temporaryRoot = path.join(repoRoot, '.temp');
  await mkdir(temporaryRoot, { recursive: true });
  assert.ok((await lstat(temporaryRoot)).isDirectory(), '.temp必须是普通目录。');
  const workingRoot = await mkdtemp(path.join(temporaryRoot, 'manual-images-'));
  for (const entry of verified) {
    const destination = path.join(workingRoot, entry.file);
    await mkdir(path.dirname(destination), { recursive: true });
    await writeFile(destination, entry.bytes, { flag: 'wx' });
  }
  console.log('测试副本：' + workingRoot);
  console.log('需要恢复时再次运行pnpm samples:prepare，新目录将包含同一套完整原件。');
}
