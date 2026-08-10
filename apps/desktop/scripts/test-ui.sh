#!/usr/bin/env bash
set -euo pipefail

npm run build:renderer -w @paper-float-translator/desktop
npx playwright test
