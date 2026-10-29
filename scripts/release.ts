// Publishes the version in package.json as the GitHub release the installed apps update from: builds the app, signs
// the update archive, and uploads it with `latest.json`, the file the updater reads (`plugins.updater` in
// `tauri.conf.json`). `--dry-run` builds and writes both files without publishing anything.
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'

const root = join(import.meta.dirname, '..')
const dryRun = process.argv.includes('--dry-run')

function fail(message: string): never {
  console.error(message)
  process.exit(1)
}

function run(command: string, args: string[]): string {
  return execFileSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  }).trim()
}

function succeeds(command: string, args: string[]): boolean {
  try {
    run(command, args)
    return true
  } catch {
    return false
  }
}

const readJson = (path: string) => JSON.parse(readFileSync(join(root, path), 'utf8'))
const { version } = readJson('package.json') as { version: string }
const config = readJson('src-tauri/tauri.conf.json') as {
  productName: string
  plugins: { updater: { endpoints: string[] } }
}
const endpoint = config.plugins.updater.endpoints[0] ?? ''
const repo =
  /^https:\/\/github\.com\/([^/]+\/[^/]+)\/releases\/latest\/download\/latest\.json$/.exec(
    endpoint,
  )?.[1] ?? fail(`The updater endpoint is not the latest GitHub release: ${endpoint}`)
const tag = `v${version}`
const arch =
  ({ arm64: 'aarch64', x64: 'x86_64' } as Record<string, string>)[process.arch] ??
  fail(`No updater platform for ${process.arch}.`)

// The installed apps only take a newer version, and the release is tagged on the commit it was built from.
if (!dryRun) {
  if (run('git', ['status', '--porcelain']))
    fail('Commit your changes first: the release is built from HEAD.')
  run('git', ['fetch', '--quiet', '--tags', 'origin', 'main'])
  if (!succeeds('git', ['merge-base', '--is-ancestor', 'HEAD', 'origin/main'])) {
    fail('Push HEAD to main first: the release is tagged on it.')
  }
  if (succeeds('gh', ['release', 'view', tag, '--repo', repo])) {
    fail(`${tag} is already released: bump "version" in package.json.`)
  }
}

// From `pnpm tauri signer generate -w ~/.tauri/claude-jobs.key`; its public key is the one in tauri.conf.json. The
// variable holds the key itself or its path; CI passes an unset secret as empty.
const key = process.env.TAURI_SIGNING_PRIVATE_KEY || join(homedir(), '.tauri', 'claude-jobs.key')
if (!process.env.TAURI_SIGNING_PRIVATE_KEY && !existsSync(key)) {
  fail(
    `No signing key at ${key}: set TAURI_SIGNING_PRIVATE_KEY to the key the app's public key belongs to.`,
  )
}
execFileSync(
  'pnpm',
  [
    'tauri',
    'build',
    '--bundles',
    'app',
    '--config',
    JSON.stringify({ bundle: { createUpdaterArtifacts: true } }),
  ],
  {
    cwd: root,
    stdio: 'inherit',
    env: {
      ...process.env,
      TAURI_SIGNING_PRIVATE_KEY: key,
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? '',
    },
  },
)

// No space in the asset name: GitHub would rename it. No version either, so the install command in the README
// always fetches the latest one.
const bundle = join(root, 'src-tauri/target/release/bundle/macos')
const out = join(root, 'src-tauri/target/release/bundle/release')
const asset = `${config.productName.replaceAll(' ', '-')}_${arch}.app.tar.gz`
rmSync(out, { recursive: true, force: true })
mkdirSync(out, { recursive: true })
copyFileSync(join(bundle, `${config.productName}.app.tar.gz`), join(out, asset))

const previous = succeeds('git', ['describe', '--tags', '--abbrev=0'])
  ? run('git', ['describe', '--tags', '--abbrev=0'])
  : null
const notes = run('git', ['log', '--format=- %s', previous ? `${previous}..HEAD` : 'HEAD'])
const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    [`darwin-${arch}`]: {
      signature: readFileSync(join(bundle, `${config.productName}.app.tar.gz.sig`), 'utf8'),
      url: `https://github.com/${repo}/releases/download/${tag}/${asset}`,
    },
  },
}
writeFileSync(join(out, 'latest.json'), `${JSON.stringify(latest, null, 2)}\n`)

if (dryRun) {
  console.log(
    `\nDry run: ${asset} and latest.json for ${tag} are in ${out}. Nothing was published.`,
  )
  process.exit(0)
}
execFileSync(
  'gh',
  [
    'release',
    'create',
    tag,
    join(out, asset),
    join(out, 'latest.json'),
    '--repo',
    repo,
    '--target',
    run('git', ['rev-parse', 'HEAD']),
    '--title',
    `${config.productName} ${version}`,
    '--notes',
    notes,
  ],
  { cwd: root, stdio: 'inherit' },
)
run('git', ['fetch', '--quiet', '--tags', 'origin'])
console.log(`\nReleased ${tag}: installed apps offer it at their next check.`)
