import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export const REPOSITORY = 'jackbrumley/steelgum';
export const INSTALL_SITE = 'https://brumley.info/steelgum/';
export const DESKTOP_MANIFEST = 'apps/steelgum-gpui/Cargo.toml';
export const PLAYER_MANIFEST = 'apps/door-game-bevy/Cargo.toml';

const manifest = readFileSync(new URL(`../${DESKTOP_MANIFEST}`, import.meta.url), 'utf8');
const packageSection = manifest.match(/^\[package\]\r?\n([\s\S]+?)(?=^\[|(?![\s\S]))/m)?.[1];
const version = packageSection?.match(/^version\s*=\s*"((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))"\s*$/m)?.[1];
if (!version) throw new Error('The Steelgum desktop Cargo package needs a plain MAJOR.MINOR.PATCH version.');

export const VERSION = version;
export const TAG = `v${VERSION}`;
export const ASSET_NAMES = Object.freeze({
  deb: `steelgum-${VERSION}-linux-x64.deb`,
  rpm: `steelgum-${VERSION}-linux-x64.rpm`,
  linux: `steelgum-${VERSION}-linux-x64.AppImage`,
  windows: `steelgum-${VERSION}-windows-x64-setup.exe`,
});
export const EXPECTED_ASSETS = Object.freeze(Object.values(ASSET_NAMES));
export const DOWNLOAD_LINKS = Object.freeze(Object.fromEntries(
  Object.entries(ASSET_NAMES).map(([platform, name]) => [
    platform, `https://github.com/${REPOSITORY}/releases/download/${TAG}/${name}`,
  ]),
));

export function releaseDownloads() {
  return [
    '### Downloads',
    '',
    `- [Linux x64 .deb (Debian, Ubuntu, Mint)](${DOWNLOAD_LINKS.deb})`,
    `- [Linux x64 .rpm (Fedora)](${DOWNLOAD_LINKS.rpm})`,
    `- [Linux x64 AppImage](${DOWNLOAD_LINKS.linux})`,
    `- [Windows x64 user installer](${DOWNLOAD_LINKS.windows})`,
  ].join('\n');
}

export function releaseNotes() {
  return [
    `# Steelgum ${TAG}`,
    '',
    'TODO',
    '',
    '### Changes',
    '',
    '- TODO',
    '',
    '---',
    '',
    releaseDownloads(),
    '',
  ].join('\n');
}

export function verifyPrerelease(release) {
  if (release.tag_name !== TAG || release.prerelease !== true || release.draft !== false) {
    throw new Error(`Expected the published prerelease ${TAG}.`);
  }
  const expected = [...EXPECTED_ASSETS].sort();
  const actual = release.assets.map((asset) => asset.name).sort();
  if (JSON.stringify(expected) !== JSON.stringify(actual)) {
    throw new Error(`Asset set mismatch. Expected ${expected.join(', ')}. Found ${actual.join(', ')}.`);
  }
  for (const asset of release.assets) {
    if (asset.state !== 'uploaded' || !/^sha256:[0-9a-f]{64}$/i.test(asset.digest ?? '')) {
      throw new Error(`Asset ${asset.name} is missing its uploaded SHA-256 digest.`);
    }
  }
}

function run(args) {
  const [command, value, ...extras] = args;
  if (extras.length === 0) {
    if (command === 'version' && !value) return console.log(VERSION);
    if (command === 'tag' && !value) return console.log(TAG);
    if (command === 'repo' && !value) return console.log(REPOSITORY);
    if (command === 'desktop-manifest' && !value) return console.log(DESKTOP_MANIFEST);
    if (command === 'player-manifest' && !value) return console.log(PLAYER_MANIFEST);
    if (command === 'assets' && !value) return console.log(EXPECTED_ASSETS.join('\n'));
    if (command === 'asset' && Object.hasOwn(ASSET_NAMES, value)) return console.log(ASSET_NAMES[value]);
    if (command === 'notes' && !value) return process.stdout.write(releaseNotes());
    if (command === 'verify' && value) return verifyPrerelease(JSON.parse(readFileSync(value, 'utf8')));
  }
  throw new Error('Usage: node scripts/release-assets.mjs version|tag|repo|desktop-manifest|player-manifest|assets|asset <deb|rpm|linux|windows>|notes|verify path');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    run(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
