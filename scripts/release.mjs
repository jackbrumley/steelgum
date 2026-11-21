import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

import { DESKTOP_MANIFEST, EXPECTED_ASSETS, INSTALL_SITE, REPOSITORY, TAG, VERSION, releaseDownloads, verifyPrerelease } from './release-assets.mjs';

const ROOT = new URL('../', import.meta.url);
const DEV = 'dev';
const WORKFLOW = '.github/workflows/release.yml';
const FILES = ['Cargo.toml', DESKTOP_MANIFEST, 'Cargo.lock', DESKTOP_MANIFEST.replace(/\.toml$/, '.lock')];
const PLAIN_VERSION = /^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$/;
const CARGO_VERSION = /^(version = ")[^"]+(")$/gm;

function output(command, args) {
  return execFileSync(command, args, { cwd: ROOT, encoding: 'utf8' }).trim();
}

function run(command, args) {
  execFileSync(command, args, { cwd: ROOT, stdio: 'inherit' });
}

function git(...args) {
  return output('git', args);
}

function ghApi(path) {
  return JSON.parse(output('gh', ['api', `repos/${REPOSITORY}/${path}`]));
}

export function validateVersion(next, current) {
  if (!PLAIN_VERSION.test(next ?? '')) throw new Error('Expected a plain X.Y.Z version.');
  const previous = current.split('.').map(BigInt);
  const candidate = next.split('.').map(BigInt);
  for (let index = 0; index < previous.length; index += 1) {
    if (candidate[index] > previous[index]) return;
    if (candidate[index] < previous[index]) break;
  }
  throw new Error(`Version ${next} must be newer than ${current}.`);
}

function assertCleanDev() {
  if (git('branch', '--show-current') !== DEV) throw new Error('Check out dev first.');
  if (git('status', '--porcelain', '--untracked-files=normal')) {
    throw new Error('Commit or move all working changes before releasing.');
  }
  const urls = [
    ...git('remote', 'get-url', '--all', 'origin').split('\n'),
    ...git('remote', 'get-url', '--push', '--all', 'origin').split('\n'),
  ];
  const valid = ['https://github.com/', 'git@github.com:', 'ssh://git@github.com/'].flatMap((prefix) => [
    `${prefix}${REPOSITORY}`, `${prefix}${REPOSITORY}.git`,
  ]).map((url) => url.toLowerCase());
  if (urls.some((url) => !valid.includes(url.toLowerCase()))) {
    throw new Error(`Both origin URLs must point to GitHub repository ${REPOSITORY}.`);
  }
}

function assertAncestor(ancestor, descendant, message) {
  try {
    git('merge-base', '--is-ancestor', ancestor, descendant);
  } catch (error) {
    if (error.status !== 1) throw error;
    const commits = git('log', '--oneline', `${descendant}..${ancestor}`);
    throw new Error(commits ? `${message}\n${commits}` : message);
  }
}

function remoteTag(tag) {
  const refs = git('ls-remote', '--tags', 'origin', `refs/tags/${tag}`, `refs/tags/${tag}^{}`);
  const ids = new Map(refs.split('\n').filter(Boolean).map((line) => {
    const [sha, name] = line.split('\t');
    return [name, sha];
  }));
  return ids.get(`refs/tags/${tag}^{}`) ?? ids.get(`refs/tags/${tag}`);
}

function prepare(next) {
  validateVersion(next, VERSION);
  assertCleanDev();
  const tag = `v${next}`;
  if (git('tag', '--list', tag) || remoteTag(tag)) throw new Error(`Tag ${tag} already exists.`);
  run('git', ['fetch', '--no-tags', 'origin', 'dev']);
  assertAncestor('FETCH_HEAD', 'HEAD', 'Local dev is missing commits from origin/dev.');

  const snapshot = FILES.map((name) => {
    const file = new URL(name, ROOT);
    return existsSync(file) ? readFileSync(file) : null;
  });
  const changes = snapshot.slice(0, 2).map((bytes, index) => {
    const content = bytes?.toString('utf8') ?? '';
    const matches = [...content.matchAll(CARGO_VERSION)];
    if (matches.length !== 1 || matches[0][0] !== `version = "${VERSION}"`) {
      throw new Error(`Expected exactly one version field matching ${VERSION} in ${FILES[index]}.`);
    }
    return content.replace(CARGO_VERSION, `version = "${next}"`);
  });

  const originalCommit = git('rev-parse', 'HEAD');
  let staged = false;
  try {
    for (const [index, content] of changes.entries()) {
      writeFileSync(new URL(FILES[index], ROOT), content);
    }
    for (const manifest of FILES.slice(0, 2)) {
      run('cargo', ['+1.99.1', 'update', '--workspace', '--manifest-path', manifest]);
    }
    for (const lockfile of FILES.slice(2)) {
      if (!existsSync(new URL(lockfile, ROOT))) throw new Error(`Cargo did not write ${lockfile}.`);
    }
    staged = true;
    run('git', ['add', '--', ...FILES]);
    run('git', ['commit', '--only', '-m', `chore: bump version to ${next}`, '--', ...FILES]);
    run('git', ['tag', tag]);
  } catch (error) {
    if (git('rev-parse', 'HEAD') === originalCommit) {
      if (staged) run('git', ['reset', '--quiet', 'HEAD', '--', ...FILES]);
      for (const [index, bytes] of snapshot.entries()) {
        const file = new URL(FILES[index], ROOT);
        if (bytes === null) rmSync(file, { force: true });
        else writeFileSync(file, bytes);
      }
    }
    throw error;
  }

  console.log(`Prepared ${tag}. Start the release workflow with git push origin dev ${tag}.`);
}

function verify() {
  const release = ghApi(`releases/tags/${TAG}`);
  verifyPrerelease(release);
  console.log(`${TAG} has verified SHA-256 metadata for ${EXPECTED_ASSETS.join(', ')}.`);
  return release;
}

export function assertNotes(release) {
  const body = release.body?.replaceAll('\r\n', '\n').trim() ?? '';
  if (release.name !== TAG || !body.startsWith(`# Steelgum ${TAG}\n\n`) || /\bTODO\b/i.test(body)) {
    throw new Error(`Finish the release notes for ${TAG} before promotion.`);
  }
  if (!body.endsWith(releaseDownloads())) throw new Error('Keep the generated Downloads block at the end of the release notes.');
}

export function assertWorkflow(runs, sha) {
  const expectedTitle = `Steelgum refs/tags/${TAG} at ${sha}`;
  const newest = runs.filter((run) => (
    run.event === 'push'
    && run.head_sha === sha
    && run.display_title === expectedTitle
    && run.path?.split('@')[0] === WORKFLOW
  )).sort((left, right) => right.run_number - left.run_number)[0];
  if (!newest || newest.status !== 'completed' || newest.conclusion !== 'success') {
    throw new Error(`The release workflow for ${TAG} at ${sha} must finish successfully before promotion.`);
  }
  return newest;
}

export async function assertInstallerDeployment(sha, readSource, fetcher = fetch) {
  const files = ['install.sh', 'uninstall.sh', 'install.ps1', 'index.html'];
  await Promise.all(files.map(async (name) => {
    const url = new URL(name === 'index.html' ? './' : name, INSTALL_SITE);
    url.searchParams.set('release', sha);
    url.searchParams.set('check', String(Date.now()));
    const expected = readSource(`docs/${name}`).replaceAll('\r\n', '\n');
    try {
      const response = await fetcher(url, {
        cache: 'no-store',
        redirect: 'error',
        signal: AbortSignal.timeout(15_000),
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const served = (await response.text()).replaceAll('\r\n', '\n');
      if (served !== expected) throw new Error('the served file differs from the verified tag');
    } catch (error) {
      throw new Error(`Installer deployment for ${name} is not ready. Wait for GitHub Pages and rerun promotion. ${error.message}`);
    }
  }));
}

async function promote() {
  assertCleanDev();
  const release = verify();
  assertNotes(release);
  const sha = git('rev-parse', `refs/tags/${TAG}^{commit}`);
  if (remoteTag(TAG) !== sha) throw new Error(`Origin must have the same ${TAG} commit as the checkout.`);

  run('git', ['fetch', '--no-tags', 'origin', '+refs/heads/dev:refs/remotes/origin/dev', '+refs/heads/main:refs/remotes/origin/main']);
  assertAncestor(sha, 'origin/dev', `Tag ${TAG} must refer to a commit on origin/dev.`);
  assertAncestor('origin/main', sha, `Origin main cannot fast-forward to ${TAG}. Bring these commits onto dev first.`);

  const { workflow_runs: runs } = ghApi(`actions/runs?event=push&head_sha=${sha}&per_page=100`);
  const workflow = assertWorkflow(runs, sha);
  console.log(`Verified successful release workflow ${workflow.html_url}.`);

  // Make the validated installer sources available on main before turning on
  // the release that references them. The non-force push also guards races.
  run('git', ['push', '--no-follow-tags', 'origin', `${sha}:refs/heads/main`]);
  await assertInstallerDeployment(sha, (name) => execFileSync(
    'git', ['show', `${sha}:${name}`], { cwd: ROOT, encoding: 'utf8' },
  ));
  if (git('ls-remote', 'origin', 'refs/heads/main').split('\t')[0] !== sha || remoteTag(TAG) !== sha) {
    throw new Error('Origin changed while promoting. The prerelease is still unavailable through releases/latest.');
  }
  const current = ghApi(`releases/tags/${TAG}`);
  verifyPrerelease(current);
  assertNotes(current);
  run('gh', ['release', 'edit', TAG, '--repo', REPOSITORY, '--prerelease=false', '--latest']);
  console.log(`${TAG} is latest. Origin main points to ${sha}.`);
}

async function main([command, ...args]) {
  if (command === 'prepare' && args.length === 1) prepare(args[0]);
  else if (command === 'verify' && args.length === 0) verify();
  else if (command === 'promote' && args.length === 0) await promote();
  else throw new Error('Usage: npm run release:prepare -- X.Y.Z | npm run release:verify | npm run release:promote');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    await main(process.argv.slice(2));
  } catch (error) {
    console.error(`Release: ${error.message}`);
    process.exitCode = 1;
  }
}
