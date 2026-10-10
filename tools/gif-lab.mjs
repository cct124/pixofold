// GIF首轮显式实验入口，当前不开放产品GIF能力。
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
} else throw new Error('参数：build [--cmake PATH] / check / core-check');
