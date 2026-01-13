#!/usr/bin/env node

/**
 * Postinstall script for llmcc
 *
 * Downloads the platform-specific native binary from GitHub releases.
 */

import { existsSync, mkdirSync, chmodSync, createWriteStream, unlinkSync, readFileSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { platform, arch } from 'os';
import { get } from 'https';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '..');
const binDir = join(projectRoot, 'bin');

// Platform detection
const platformMap = {
  'darwin-arm64': 'llmcc-darwin-arm64',
  'darwin-x64': 'llmcc-darwin-x64',
  'linux-arm64': 'llmcc-linux-arm64',
  'linux-x64': 'llmcc-linux-x64',
  'win32-x64': 'llmcc-win32-x64.exe',
};

const platformKey = `${platform()}-${arch()}`;
const binaryName = platformMap[platformKey];

if (!binaryName) {
  console.log(`⚠ No pre-built binary available for ${platformKey}`);
  console.log('  You can build from source: https://github.com/allenanswerzq/llmcc');
  process.exit(0);
}

const binaryPath = join(binDir, binaryName);

// Package info
const packageJson = JSON.parse(readFileSync(join(projectRoot, 'package.json'), 'utf8'));
const version = packageJson.version;

// GitHub release URL
const GITHUB_REPO = 'allenanswerzq/llmcc';
const DOWNLOAD_URL = `https://github.com/${GITHUB_REPO}/releases/download/v${version}/${binaryName}`;

async function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest);

    const request = (url) => {
      get(url, (response) => {
        // Handle redirects (GitHub uses them)
        if (response.statusCode === 301 || response.statusCode === 302) {
          request(response.headers.location);
          return;
        }

        if (response.statusCode !== 200) {
          reject(new Error(`HTTP ${response.statusCode}`));
          return;
        }

        response.pipe(file);
        file.on('finish', () => {
          file.close();
          resolve();
        });
      }).on('error', (err) => {
        if (existsSync(dest)) unlinkSync(dest);
        reject(err);
      });
    };

    request(url);
  });
}

async function main() {
  // Check if binary already exists
  if (existsSync(binaryPath)) {
    console.log(`✓ llmcc binary already exists`);
    return;
  }

  // Ensure bin directory exists
  if (!existsSync(binDir)) {
    mkdirSync(binDir, { recursive: true });
  }

  console.log(`Downloading llmcc v${version} for ${platformKey}...`);

  try {
    await downloadFile(DOWNLOAD_URL, binaryPath);

    // Make executable on Unix
    if (platform() !== 'win32') {
      chmodSync(binaryPath, 0o755);
    }

    console.log(`✓ Downloaded llmcc binary`);
  } catch (err) {
    console.log(`⚠ Could not download binary: ${err.message}`);
    console.log('');
    console.log('You can install llmcc via other methods:');
    console.log('  • cargo install llmcc');
    console.log('  • Download from: https://github.com/allenanswerzq/llmcc/releases');
    console.log('');
  }
}

main().catch(console.error);
