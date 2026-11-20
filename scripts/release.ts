// Publishes the version in package.json as the GitHub release the installed apps update from, in two steps that
// `.github/workflows/release.yml` runs:
//
// - `build`, on each platform: builds the app, signs its update archive, and leaves it in the release folder with a
//   `<platform>.json` that says where the release will serve it and its signature. Alone, it publishes nothing.
// - `publish <folder>`: merges every platform's file into `latest.json`, the file the updater reads
//   (`plugins.updater` in `tauri.conf.json`), and creates the release with it and the archives in `<folder>`.
import { execFileSync } from 'node:child_process'
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'

const root = join(import.meta.dirname, '..')

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
// No space in an asset's name: GitHub would rename it.
const name = config.productName.replaceAll(' ', '-')
const out = join(root, 'src-tauri/target/release/bundle/release')

type Platform = { signature: string; url: string }

function build(): void {
  const arch =
    ({ arm64: 'aarch64', x64: 'x86_64' } as Record<string, string>)[process.arch] ??
    fail(`No updater platform for ${process.arch}.`)
  // The macOS app as a .tar.gz, and on Linux the AppImage itself, each with its signature beside it.
  const target =
    process.platform === 'darwin'
      ? { platform: `darwin-${arch}`, bundles: 'app', folder: 'macos', extension: '.app.tar.gz' }
      : process.platform === 'linux'
        ? {
            platform: `linux-${arch}`,
            bundles: 'appimage',
            folder: 'appimage',
            extension: '.AppImage',
          }
        : fail(`No release for ${process.platform}.`)

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
      target.bundles,
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

  const bundle = join(root, 'src-tauri/target/release/bundle', target.folder)
  const built =
    readdirSync(bundle).find((file) => file.endsWith(target.extension)) ??
    fail(`No ${target.extension} in ${bundle}.`)
  // No version in the name, so the install commands in the README always fetch the latest one.
  const asset = `${name}_${arch}${target.extension}`
  rmSync(out, { recursive: true, force: true })
  mkdirSync(out, { recursive: true })
  copyFileSync(join(bundle, built), join(out, asset))
  const platform: Platform = {
    signature: readFileSync(join(bundle, `${built}.sig`), 'utf8'),
    url: `https://github.com/${repo}/releases/download/${tag}/${asset}`,
  }
  writeFileSync(
    join(out, `${target.platform}.json`),
    `${JSON.stringify({ [target.platform]: platform }, null, 2)}\n`,
  )
  console.log(`\nBuilt ${asset} for ${tag} in ${out}. Nothing was published.`)
}

function publish(folder: string): void {
  // The installed apps only take a newer version, and the release is tagged on the commit it was built from.
  if (run('git', ['status', '--porcelain']))
    fail('Commit your changes first: the release is built from HEAD.')
  run('git', ['fetch', '--quiet', '--tags', 'origin', 'main'])
  if (!succeeds('git', ['merge-base', '--is-ancestor', 'HEAD', 'origin/main'])) {
    fail('Push HEAD to main first: the release is tagged on it.')
  }
  if (succeeds('gh', ['release', 'view', tag, '--repo', repo])) {
    fail(`${tag} is already released: bump "version" in package.json.`)
  }

  const files = readdirSync(folder).filter((file) => file !== 'latest.json')
  const platforms: Record<string, Platform> = Object.assign(
    {},
    ...files
      .filter((file) => file.endsWith('.json'))
      .map((file) => JSON.parse(readFileSync(join(folder, file), 'utf8'))),
  )
  if (Object.keys(platforms).length === 0) fail(`No platform was built in ${folder}.`)
  const assets = files.filter((file) => !file.endsWith('.json')).map((file) => join(folder, file))

  const previous = succeeds('git', ['describe', '--tags', '--abbrev=0'])
    ? run('git', ['describe', '--tags', '--abbrev=0'])
    : null
  const notes = run('git', ['log', '--format=- %s', previous ? `${previous}..HEAD` : 'HEAD'])
  const latest = { version, notes, pub_date: new Date().toISOString(), platforms }
  writeFileSync(join(folder, 'latest.json'), `${JSON.stringify(latest, null, 2)}\n`)

  execFileSync(
    'gh',
    [
      'release',
      'create',
      tag,
      ...assets,
      join(folder, 'latest.json'),
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
  console.log(
    `\nReleased ${tag} for ${Object.keys(platforms).join(', ')}: installed apps offer it at their next check.`,
  )
}

const [command, folder] = process.argv.slice(2)
if (command === 'build') build()
else if (command === 'publish' && folder) publish(folder)
else fail('usage: pnpm release build | pnpm release publish <folder>')
