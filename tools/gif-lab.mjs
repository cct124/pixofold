// GIF显式实验/验证入口；有损仍属开发实验，产品能力由桌面契约决定。
import { buildTools } from './gif-lab/build.mjs';
const [command, ...args] = process.argv.slice(2);
if (command === 'build') {
  if (args.length && !(args.length === 2 && args[0] === '--cmake'))
    throw new Error('build仅接受--cmake PATH');
  await buildTools(args[1]);
} else if (command === 'check' && args.length === 0) {
  const { runExperiment } = await import('./gif-lab/experiment.mjs');
  runExperiment();
} else if (command === 'core-check' && args.length === 0) {
  const { runCoreCheck } = await import('./gif-lab/core-check.mjs');
  runCoreCheck();
} else if (command === 'lossy-check' && args.length === 0) {
  const { runLossyExperiment } = await import('./gif-lab/lossy-check.mjs');
  runLossyExperiment();
} else if (command === 'lossy-sources-check' && args.length === 0) {
  const { checkLossySources } = await import('./gif-lab/lossy-check.mjs');
  checkLossySources();
  console.log('GIF有损实验的公开录屏与许可SHA256通过。');
} else
  throw new Error(
    '参数：build [--cmake PATH] / check / core-check / lossy-check / lossy-sources-check',
  );
