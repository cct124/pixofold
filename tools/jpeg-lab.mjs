// JPEG实验显式入口；生产构建复用同一配方，实验工具不随应用分发。
import {
  root,
  engine,
  source,
  build,
  buildIdentity,
  buildTools,
  verifyTools,
} from './jpeg-lab/build.mjs';
import { runExperiment } from './jpeg-lab/experiment.mjs';

const [command, ...args] = process.argv.slice(2);
if (!['build', 'check', 'core-check'].includes(command))
  throw new Error('参数：build [--cmake PATH]、check或core-check');
if (args.length && !(command === 'build' && args.length === 2 && args[0] === '--cmake'))
  throw new Error('仅build接受--cmake PATH');
if (command === 'build') await buildTools(args[1]);
else {
  const tools = verifyTools();
  if (command === 'check') runExperiment({ root, engine, buildIdentity, tools });
  else {
    const { runCoreChecks } = await import('./jpeg-lab/core-check.mjs');
    runCoreChecks({ root, build, tools, source });
  }
}
