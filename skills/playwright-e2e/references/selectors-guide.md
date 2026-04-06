# Playwright Selectors Guide

## Selector Priority (Best → Worst)

| Priority | Selector | Example | Why |
|----------|----------|---------|-----|
| 1st | `getByRole()` | `page.getByRole('button', { name: 'Submit' })` | Semantic, accessible |
| 2nd | `getByLabel()` | `page.getByLabel('Email')` | Form-specific, accessible |
| 3rd | `getByText()` | `page.getByText('Welcome back')` | Content-based |
| 4th | `getByTestId()` | `page.getByTestId('submit-btn')` | Explicit test hook |
| 5th | `getByPlaceholder()` | `page.getByPlaceholder('Search...')` | Input-specific |
| Avoid | CSS/XPath | `page.locator('.btn-primary')` | Brittle, not semantic |

## Common Patterns

### Forms
```typescript
// Fill a form
await page.getByLabel('Email').fill('user@example.com')
await page.getByLabel('Password').fill('secret123')
await page.getByRole('button', { name: 'Sign in' }).click()

// Select dropdown
await page.getByLabel('Country').selectOption('US')

// Checkbox
await page.getByRole('checkbox', { name: 'Remember me' }).check()
```

### Navigation
```typescript
await page.getByRole('link', { name: 'Dashboard' }).click()
await page.waitForURL('/dashboard')
```

### Tables
```typescript
const row = page.getByRole('row', { name: /john@example/ })
await row.getByRole('button', { name: 'Edit' }).click()
```

### Lists
```typescript
const items = page.getByRole('listitem')
await expect(items).toHaveCount(5)
await items.nth(0).click()
```

### Dialogs
```typescript
const dialog = page.getByRole('dialog')
await expect(dialog).toBeVisible()
await dialog.getByRole('button', { name: 'Confirm' }).click()
```

## Assertions

```typescript
// Visibility
await expect(locator).toBeVisible()
await expect(locator).toBeHidden()

// Text content
await expect(locator).toHaveText('exact text')
await expect(locator).toContainText('partial')

// Attributes
await expect(locator).toHaveAttribute('aria-invalid', 'true')

// URL
await expect(page).toHaveURL('/dashboard')

// Count
await expect(page.getByRole('listitem')).toHaveCount(3)
```

## Waiting Strategies

```typescript
// Wait for element
await page.getByTestId('loading').waitFor({ state: 'hidden' })

// Wait for network
await page.waitForResponse(resp =>
  resp.url().includes('/api/users') && resp.status() === 200
)

// Wait for navigation
await Promise.all([
  page.waitForURL('/dashboard'),
  page.getByRole('button', { name: 'Login' }).click(),
])
```
