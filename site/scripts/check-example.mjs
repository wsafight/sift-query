import {mkdtemp, mkdir, readFile, rm, writeFile} from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';

const root = fileURLToPath(new URL('../../', import.meta.url));
const run = promisify(execFile);
async function codeBlock(file, language) {
  const markdown = await readFile(path.join(root, file), 'utf8');
  const block = markdown.match(new RegExp('```' + language + '\\n([\\s\\S]*?)\\n```'))?.[1];
  if (!block) throw new Error(`Missing ${language} example in ${file}`);
  return block;
}

const rust = await codeBlock('docs/guides/en/quick-start.md', 'rust');
const toml = await codeBlock('docs/guides/en/installation.md', 'toml');
if (rust !== await codeBlock('docs/guides/zh/quick-start.md', 'rust') ||
    toml !== await codeBlock('docs/guides/zh/installation.md', 'toml')) {
  throw new Error('English and Chinese executable examples differ');
}

const project = await mkdtemp(path.join(os.tmpdir(), 'sift-query-doc-example-'));
try {
  await mkdir(path.join(project, 'src'));
  await writeFile(path.join(project, 'src/main.rs'), `${rust}\n`);
  await writeFile(path.join(project, 'Cargo.toml'), `[package]\nname = "sift-doc-example"\nversion = "0.1.0"\nedition = "2024"\n\n${toml}\n`);
  const {stdout} = await run('cargo', ['run', '--quiet'], {
    cwd: project,
    env: {...process.env, CARGO_TARGET_DIR: path.join(root, 'target/docs-example')},
    timeout: 600_000,
  });
  if (stdout.trim() !== '1: Alice\n3: Ari') throw new Error(`Unexpected SQLite example output: ${stdout}`);
  console.log('Both language guides compile and query successfully using the published crates.');
} finally {
  await rm(project, {recursive: true, force: true});
}
