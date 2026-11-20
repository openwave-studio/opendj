#!/usr/bin/env bash
# Builds opendj.app into dist/: the landing pages as they are, plus the
# Starlight docs (docs/) in dist/docs. Cloudflare Pages runs this with the
# project's root directory set to website/ and the output directory set to
# dist; its build image has Node (version in .node-version).
set -euo pipefail
cd "$(dirname "$0")"

rm -rf dist
mkdir -p dist
cp -R index.html 404.html style.css home.css main.js _headers assets tutorials about releases dist/

cd docs
npm ci --no-audit --no-fund
npm run build
cd ..
echo "Built $(find dist -type f | wc -l) files into website/dist"
