#!/usr/bin/env node
const { spawnSync } = require('node:child_process')
const { join } = require('node:path')

const map = {
  'darwin-arm64': 'cb-darwin',
  'darwin-x64': 'cb-darwin',
  'linux-x64': 'cb-linux-x64',
  'linux-arm64': 'cb-linux-arm64',
}

const key = `${process.platform}-${process.arch}`
const name = map[key]
if (!name) {
  console.error(`cb: unsupported platform ${key}. Supported: ${Object.keys(map).join(', ')}`)
  process.exit(1)
}

const bin = join(__dirname, name)
const r = spawnSync(bin, process.argv.slice(2), { stdio: 'inherit' })
if (r.error) {
  console.error(r.error.message)
  process.exit(1)
}
process.exit(r.status ?? 1)
