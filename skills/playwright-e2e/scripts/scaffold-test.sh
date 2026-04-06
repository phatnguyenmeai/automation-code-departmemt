#!/bin/bash
# Scaffold a new Playwright test spec with Page Object.
# Usage: ./scaffold-test.sh <PageName>
#
# Creates:
#   tests/e2e/pages/<PageName>Page.ts
#   tests/e2e/specs/<page-name>/<page-name>.spec.ts

set -euo pipefail

PAGE_NAME="${1:?Usage: scaffold-test.sh <PageName>}"
LOWER_NAME=$(echo "$PAGE_NAME" | sed 's/\([A-Z]\)/-\L\1/g' | sed 's/^-//')

PAGES_DIR="tests/e2e/pages"
SPECS_DIR="tests/e2e/specs/${LOWER_NAME}"

mkdir -p "$PAGES_DIR" "$SPECS_DIR"

# Page Object
cat > "$PAGES_DIR/${PAGE_NAME}Page.ts" << EOF
import { type Page, type Locator } from '@playwright/test'
import { BasePage } from './BasePage'

export class ${PAGE_NAME}Page extends BasePage {
  readonly path = '/${LOWER_NAME}'

  // Locators — use role-based selectors for resilience
  get heading(): Locator {
    return this.page.getByRole('heading', { name: '${PAGE_NAME}' })
  }

  // TODO: Add page-specific locators using:
  //   this.page.getByRole('button', { name: '...' })
  //   this.page.getByLabel('...')
  //   this.page.getByTestId('...')
  //   this.page.getByText('...')

  // TODO: Add page-specific actions
}
EOF

# Test spec
cat > "$SPECS_DIR/${LOWER_NAME}.spec.ts" << EOF
import { test, expect } from '@playwright/test'
import { ${PAGE_NAME}Page } from '../../pages/${PAGE_NAME}Page'

test.describe('${PAGE_NAME}', () => {
  let page: ${PAGE_NAME}Page

  test.beforeEach(async ({ page: pwPage }) => {
    page = new ${PAGE_NAME}Page(pwPage)
    await page.goto()
  })

  test('page loads successfully', async () => {
    await expect(page.heading).toBeVisible()
  })

  // TODO: Add test cases for:
  // - Happy path scenarios
  // - Error/validation cases
  // - Edge cases
  // - Accessibility (axe-core)
})
EOF

echo "Test scaffolded:"
echo "  Page Object: $PAGES_DIR/${PAGE_NAME}Page.ts"
echo "  Test Spec:   $SPECS_DIR/${LOWER_NAME}.spec.ts"
