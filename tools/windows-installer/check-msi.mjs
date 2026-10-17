import { createHash, randomUUID } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmdirSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { root, target } from '../jpeg-lab/build.mjs';

if (process.platform !== 'win32') throw new Error('MSI verification requires Windows.');
const perMachine = process.argv.includes('--per-machine');
if (process.argv.slice(2).some((value) => value !== '--per-machine'))
  throw new Error('Usage: check-msi.mjs [--per-machine]');
if (perMachine) {
  const elevated = execFileSync(
    'powershell.exe',
    [
      '-NoProfile',
      '-Command',
      '([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)',
    ],
    { encoding: 'utf8', windowsHide: true },
  ).trim();
  if (elevated !== 'True') throw new Error('--per-machine requires an administrator terminal.');
}
const localData = execFileSync(
  'powershell.exe',
  ['-NoProfile', '-Command', "[Environment]::GetFolderPath('LocalApplicationData')"],
  { encoding: 'utf8', windowsHide: true },
).trim();
const wix = path.join(localData, 'tauri/WixTools314');
const probeDll = path.join(
  root,
  'target/windows-installer',
  target,
  'Release/pixofold-installer-probe.dll',
);
if (!existsSync(probeDll)) throw new Error('Run pnpm installer:check first.');
const evidence = mkdtempSync(path.join(root, 'target/installer-msi-'));
const profile = path.join(localData, 'app.pixofold.installer-probe');
if (existsSync(profile)) throw new Error('Probe profile already exists; preserving it: ' + profile);
const upgrade = randomUUID(),
  component = randomUUID(),
  codes = [randomUUID(), randomUUID()];
const fragment = readFileSync(path.join(root, 'src-tauri/windows/installer.wxs'), 'utf8');
const actions = fragment.match(/<CustomAction [^>]+\/>/g);
const sequence = fragment.match(/<InstallExecuteSequence>[\s\S]*?<\/InstallExecuteSequence>/)?.[0];
if (actions?.length !== 2 || !sequence) throw new Error('Unexpected production installer actions.');
const xml = (value) => value.replaceAll('&', '&amp;').replaceAll('"', '&quot;');
const results = [];
let installed = null;
function run(program, args, log) {
  const result = spawnSync(program, args, { encoding: 'utf8', windowsHide: true });
  writeFileSync(log, (result.stdout ?? '') + (result.stderr ?? ''));
  if (result.error || ![0, 3010].includes(result.status))
    throw new Error(path.basename(program) + ' failed: ' + result.status + '; see ' + log, {
      cause: result.error,
    });
}
function msi(args, name) {
  const log = path.join(evidence, name + '.log');
  run(
    path.join(process.env.SystemRoot, 'System32/msiexec.exe'),
    [...args, '/qn', '/norestart', '/L*v', log],
    path.join(evidence, name + '-process.log'),
  );
  const output = readFileSync(log);
  return output[0] === 0xff && output[1] === 0xfe
    ? output.subarray(2).toString('utf16le')
    : output.toString('utf8');
}
function build(index) {
  const name = 'probe-' + index;
  const source = path.join(evidence, name + '.wxs');
  writeFileSync(
    source,
    [
      '<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">',
      '<Product Id="' +
        codes[index] +
        '" UpgradeCode="' +
        upgrade +
        '" Name="PixoFold Data Cleanup Probe" Manufacturer="PixoFold test" Language="1033" Version="0.' +
        (index + 1) +
        '.0">',
      '<Package InstallerVersion="450" Compressed="yes" InstallScope="' +
        (perMachine ? 'perMachine' : 'perUser') +
        '" InstallPrivileges="' +
        (perMachine ? 'elevated' : 'limited') +
        '" />',
      '<MajorUpgrade Schedule="afterInstallInitialize" AllowDowngrades="yes" /><MediaTemplate EmbedCab="yes" />',
      '<Directory Id="TARGETDIR" Name="SourceDir"><Directory Id="' +
        (perMachine ? 'ProgramFiles64Folder' : 'LocalAppDataFolder') +
        '"><Directory Id="INSTALLDIR" Name="PixoFold Data Cleanup Probe">',
      '<Component Id="ProbeFile" Guid="' +
        component +
        '" Win64="yes"><File Id="ProbeText" Source="' +
        xml(path.join(evidence, 'probe.txt')) +
        '" />',
      '<RegistryValue Root="' +
        (perMachine ? 'HKLM' : 'HKCU') +
        '" Key="Software\\PixoFoldDataCleanupProbe" Name="Installed" Type="integer" Value="1" KeyPath="yes" /><RemoveFolder Id="RemoveProbeDirectory" On="uninstall" />',
      '</Component></Directory></Directory></Directory><Feature Id="Main" Level="1"><ComponentRef Id="ProbeFile" /></Feature>',
      '<Binary Id="PixoFoldInstallerActions" SourceFile="' + xml(probeDll) + '" />',
      ...actions,
      '<CustomAction Id="ProbePixoFoldUserContext" BinaryKey="PixoFoldInstallerActions" DllEntry="ProbePixoFoldUserContext" Execute="immediate" Return="check" />',
      sequence.replace(
        '</InstallExecuteSequence>',
        '<Custom Action="ProbePixoFoldUserContext" After="InstallValidate">1</Custom></InstallExecuteSequence>',
      ),
      '</Product></Wix>',
    ].join('\n'),
  );
  const object = path.join(evidence, name + '.wixobj');
  const output = path.join(evidence, name + '.msi');
  run(
    path.join(wix, 'candle.exe'),
    ['-nologo', '-arch', 'x64', '-out', object, source],
    path.join(evidence, name + '-candle.log'),
  );
  run(
    path.join(wix, 'light.exe'),
    ['-nologo', '-out', output, object],
    path.join(evidence, name + '-light.log'),
  );
  return output;
}
writeFileSync(path.join(evidence, 'probe.txt'), 'Independent MSI verification fixture.\n');
const packages = [build(0), build(1)];
const cache = path.join(profile, 'EBWebView/Default');
mkdirSync(cache, { recursive: true });
const fixtures = [
  path.join(profile, 'overwrite-consent-v1.json'),
  path.join(cache, 'preferences-fixture'),
];
for (const file of fixtures) writeFileSync(file, 'preserve this independent probe data\n');
const hashes = fixtures.map((file) =>
  createHash('sha256').update(readFileSync(file)).digest('hex'),
);
function preserved() {
  if (
    !fixtures.every(
      (file, i) =>
        existsSync(file) &&
        createHash('sha256').update(readFileSync(file)).digest('hex') === hashes[i],
    )
  )
    throw new Error('Probe app data was modified.');
}
try {
  const first = msi(['/i', packages[0]], 'install');
  installed = codes[0];
  if (!first.includes('PixoFold probe user context verified'))
    throw new Error('MSI user context not verified.');
  const quiet = msi(['/x', '{' + codes[0] + '}', 'PixoFoldCleanupRequested=1'], 'quiet-uninstall');
  installed = null;
  preserved();
  if (
    !quiet.includes('ConfirmPixoFoldDataRemoval') ||
    !quiet.includes('RemovePixoFoldDataAfterUninstall')
  )
    throw new Error('MSI did not include production uninstall actions.');
  results.push({ operation: 'quiet-uninstall-with-injected-choice', retained: true });
  msi(['/i', packages[0]], 'install-again');
  installed = codes[0];
  const upgradeLog = msi(['/i', packages[1]], 'upgrade');
  installed = codes[1];
  preserved();
  if (
    !upgradeLog.includes('UPGRADINGPRODUCTCODE') ||
    !upgradeLog.includes('ConfirmPixoFoldDataRemoval')
  )
    throw new Error('Major-upgrade removal was not observed.');
  results.push({ operation: 'major-upgrade', retained: true });
  msi(['/x', '{' + codes[1] + '}'], 'final-uninstall');
  installed = null;
  preserved();
  results.push({ operation: 'quiet-uninstall', retained: true });
  writeFileSync(
    path.join(evidence, 'result.json'),
    JSON.stringify(
      {
        scope: perMachine ? 'perMachine' : 'perUser',
        results,
        productionFragmentSha256: createHash('sha256').update(fragment).digest('hex'),
        probeDllSha256: createHash('sha256').update(readFileSync(probeDll)).digest('hex'),
      },
      null,
      2,
    ) + '\n',
  );
  // 只移除本次创建且已核对哈希的测试文件；未知文件会阻止rmdir，不递归清空用户目录。
  for (const file of fixtures) unlinkSync(file);
  for (const directory of [cache, path.dirname(cache), profile]) {
    if (readdirSync(directory).length !== 0)
      throw new Error('Unexpected probe profile content: ' + directory);
    rmdirSync(directory);
  }
  console.log('MSI isolation, user context, quiet uninstall and upgrade verified: ' + evidence);
} finally {
  if (installed) {
    try {
      msi(['/x', '{' + installed + '}'], 'failure-cleanup');
    } catch (error) {
      console.error('Probe uninstall needs attention: ' + error.message);
    }
  }
}
