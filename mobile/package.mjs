// Builds the Android app: stages the game and the engine (stage.mjs), copies
// them into the native project (cap sync) and has Gradle build a release APK,
// written to dist/release/Isorun.apk. Needs a JDK 21 and the Android SDK
// (ANDROID_HOME); the release key comes from ISORUN_KEYSTORE (README.md).
//
//   npm run package                 # the APK
//   npm run package -- --bundle     # and an Android App Bundle for Google Play, Isorun.aab
//   npm run package -- --publish    # and upload the APK to the release v<version> (gh)

import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

const mobile = import.meta.dirname
const isorun = join(mobile, '..')
const android = join(mobile, 'android')
const release = join(mobile, 'dist/release')
const bundle = process.argv.includes('--bundle')
const publish = process.argv.includes('--publish')

execFileSync('node', [join(mobile, 'stage.mjs')], { stdio: 'inherit' })
execFileSync('npx', ['cap', 'sync', 'android'], { cwd: mobile, stdio: 'inherit' })

if (!process.env.ISORUN_KEYSTORE) console.warn('ISORUN_KEYSTORE is not set: the release is signed with the debug key.')
const gradlew = join(android, process.platform === 'win32' ? 'gradlew.bat' : 'gradlew')
execFileSync(gradlew, ['--no-daemon', 'assembleRelease', ...(bundle ? ['bundleRelease'] : [])], { cwd: android, stdio: 'inherit' })

mkdirSync(release, { recursive: true })
const apk = join(release, 'Isorun.apk')
copyFileSync(join(android, 'app/build/outputs/apk/release/app-release.apk'), apk)
const artifacts = [apk]
if (bundle) {
  const aab = join(release, 'Isorun.aab')
  copyFileSync(join(android, 'app/build/outputs/bundle/release/app-release.aab'), aab)
  artifacts.push(aab)
}

if (publish) {
  const { version } = JSON.parse(readFileSync(join(isorun, 'apps/isorun/package.json'), 'utf8'))
  execFileSync('gh', ['release', 'upload', `v${version}`, apk, '--clobber', '--repo', 'maciej-rosiek/isorun'], { stdio: 'inherit' })
}

console.log(artifacts.join('\n'))
