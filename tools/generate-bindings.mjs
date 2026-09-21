import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const output = new URL('../src/lib/ipc/generated.ts', import.meta.url);
const result = spawnSync(
  'cargo',
  [
    'run',
    '--quiet',
    '--locked',
    '--package',
    'pixofold-core',
    '--features',
    'bindings',
    '--bin',
    'export-bindings',
  ],
  { cwd: root, encoding: 'utf8', windowsHide: true },
);

if (result.error || result.status !== 0) {
  console.error(result.error?.message ?? result.stderr);
  process.exit(1);
}

const content = result.stdout.replaceAll('\r\n', '\n');
if (process.argv.includes('--check')) {
  const current = await readFile(output, 'utf8').catch((error) => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
  if (current?.replaceAll('\r\n', '\n') !== content) {
    console.error('IPC 类型已过期，请运行 pnpm types:generate 并提交生成文件。');
    process.exit(1);
  }
  console.log('Rust / TypeScript 类型一致。');
} else {
  await mkdir(new URL('../src/lib/ipc/', import.meta.url), { recursive: true });
  await writeFile(output, content, 'utf8');
  console.log('已生成 src/lib/ipc/generated.ts。');
}
