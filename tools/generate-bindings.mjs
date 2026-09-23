import { spawnSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const targets = [
  ['pixofold-core', 'export-bindings', 'generated.ts'],
  ['pixofold-desktop', 'export-task-bindings', 'tasks.generated.ts'],
];
const outputs = [];
for (const [packageName, binary, file] of targets) {
  const result = spawnSync(
    'cargo',
    [
      'run',
      '--quiet',
      '--locked',
      '--package',
      packageName,
      '--features',
      'bindings',
      '--bin',
      binary,
    ],
    { cwd: root, encoding: 'utf8', windowsHide: true },
  );
  if (result.error || result.status !== 0) {
    console.error(result.error?.message ?? result.stderr);
    process.exit(1);
  }
  outputs.push({
    file,
    output: new URL('../src/lib/ipc/' + file, import.meta.url),
    content: result.stdout.replaceAll('\r\n', '\n'),
  });
}

// 先完成全部生成，再检查/写入，避免后一个编译失败留下半套类型。
if (process.argv.includes('--check')) {
  for (const { file, output, content } of outputs) {
    const current = await readFile(output, 'utf8').catch((error) => {
      if (error.code === 'ENOENT') return null;
      throw error;
    });
    if (current?.replaceAll('\r\n', '\n') !== content) {
      console.error(file + ' 已过期，请运行 pnpm types:generate 并提交生成文件。');
      process.exit(1);
    }
  }
  console.log('Rust / TypeScript 类型一致。');
} else {
  await mkdir(new URL('../src/lib/ipc/', import.meta.url), { recursive: true });
  for (const { file, output, content } of outputs) {
    await writeFile(output, content, 'utf8');
    console.log('已生成 src/lib/ipc/' + file + '。');
  }
}
