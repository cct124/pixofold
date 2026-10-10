// 固定有界helper的真实核心回归，输入/输出只在新建的隔离目录。
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
import { verifyTools as verifyJpegTools, source as jpegSource } from '../jpeg-lab/build.mjs';
import { createSamples } from '../jpeg-lab/experiment.mjs';
import { writeFixtures, stressFixture, largeFixture } from './fixtures.mjs';
export function runCoreCheck() {
  const identity = verifyTools();
  writeFixtures(path.join(root, 'tests/fixtures/gif'), true);
  assert.match(
    execute(helper, ['--memory-bytes', '4096', '--allocation-self-test'], { encoding: 'utf8' })
      .stdout,
    /bounded-allocation-passed/,
  );
  mkdirSync(path.join(root, 'target'), { recursive: true });
  const directory = mkdtempSync(path.join(root, 'target/gif-core-'));
  const inputs = path.join(directory, 'inputs');
  cpSync(path.join(root, 'tests/fixtures/gif'), inputs, { recursive: true });
  writeFileSync(path.join(inputs, 'stress.gif'), stressFixture());
  writeFileSync(path.join(inputs, 'large-static.gif'), largeFixture());
  assert.throws(
    () => execute(helper, ['--memory-bytes', '512', '--allocation-self-test']),
    (error) => error.actual === 86,
  );
  execute(
    'cargo',
    [
      'run',
      '--release',
      '--locked',
      '-p',
      'pixofold-core',
      '--example',
      'gif_core_check',
      '--',
      helperDirectory,
      identity.helperSha256,
      inputs,
    ],
    { stdio: 'inherit' },
  );
  for (const notice of ['COPYING', 'README.md'])
    cpSync(path.join(source, notice), path.join(directory, 'Gifsicle-' + notice));
  cpSync(path.join(root, 'native/gif'), path.join(directory, 'native-gif-source'), {
    recursive: true,
  });
  cpSync(path.join(root, 'tools/gif-lab/licenses'), path.join(directory, 'validator-licenses'), {
    recursive: true,
  });
  cpSync(path.join(build, 'identity.json'), path.join(directory, 'engine-identity.json'));
  assert.equal(
    JSON.parse(readFileSync(path.join(inputs, 'core-results/report.json'), 'utf8')).result,
    'passed',
  );
  const jpegTools = verifyJpegTools();
  const jpegHash = digest(readFileSync(jpegTools.helper));
  const { samples } = createSamples({ root, tools: jpegTools, directory });
  writeFileSync(
    path.join(inputs, 'baseline-420.jpg'),
    samples.find(([name]) => name === 'baseline-420')[1],
  );
  execute(
    'cargo',
    [
      'run',
      '--release',
      '--locked',
      '-p',
      'pixofold-core',
      '--example',
      'gif_mixed_check',
      '--',
      helperDirectory,
      identity.helperSha256,
      path.dirname(jpegTools.helper),
      jpegHash,
      inputs,
    ],
    { stdio: 'inherit' },
  );
  assert.equal(
    JSON.parse(readFileSync(path.join(inputs, 'mixed-results/report.json'), 'utf8')).result,
    'passed',
  );
  const jpegOutput = readFileSync(path.join(inputs, 'mixed-results/outputs/图片.JPG'));
  assert.deepEqual(
    execute(jpegTools.coefficients, [], { input: jpegOutput }).stdout,
    execute(jpegTools.coefficients, [], {
      input: readFileSync(path.join(inputs, 'baseline-420.jpg')),
    }).stdout,
  );
  for (const notice of ['LICENSE.md', 'README.ijg'])
    cpSync(path.join(jpegSource, notice), path.join(directory, 'MozJPEG-' + notice));
  console.log('GIF核心证据：' + directory);
}
