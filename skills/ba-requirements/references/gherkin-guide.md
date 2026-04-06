# Gherkin/BDD Writing Guide

## Given/When/Then Structure

| Keyword | Purpose | Example |
|---------|---------|---------|
| **Given** | Pre-conditions, setup state | Given the user is logged in |
| **When** | Action being performed | When they click "Add to Cart" |
| **Then** | Expected outcome | Then the cart count increases by 1 |
| **And** | Additional steps | And a confirmation toast is shown |
| **But** | Negative condition | But the "Checkout" button is disabled |

## Good vs Bad Acceptance Criteria

### Bad (vague, untestable)
- "The page should load fast"
- "The user experience should be good"
- "Errors should be handled properly"

### Good (specific, testable)
- "Given a product page, when loaded on 3G, then the first contentful paint is under 2 seconds"
- "Given an item in the cart, when the user clicks Remove, then the item is removed and the total is recalculated"
- "Given invalid email format, when the user submits the form, then a validation error 'Please enter a valid email' is shown below the email field"

## Scenario Templates

### CRUD Operations
```gherkin
Scenario: Create [Entity]
  Given I am on the [entity] creation page
  And I am authenticated as [role]
  When I fill in [required fields]
  And I click "Save"
  Then the [entity] is created
  And I am redirected to the [entity] detail page
  And a success notification "Entity created" is shown

Scenario: Read [Entity] List
  Given there are N [entities] in the system
  When I navigate to the [entities] list page
  Then I see N [entities] displayed
  And each [entity] shows [key fields]

Scenario: Update [Entity]
  Given an existing [entity] with id X
  When I change [field] to [new value]
  And I click "Save"
  Then the [entity] is updated
  And the [field] shows [new value]

Scenario: Delete [Entity]
  Given an existing [entity] with id X
  When I click "Delete"
  And I confirm the deletion
  Then the [entity] is removed
  And I am redirected to the [entities] list
```

### Authentication
```gherkin
Scenario: Successful Login
  Given I am on the login page
  When I enter valid email and password
  And I click "Sign in"
  Then I am redirected to the dashboard
  And I see my name in the header

Scenario: Failed Login
  Given I am on the login page
  When I enter an incorrect password
  And I click "Sign in"
  Then I see an error "Invalid credentials"
  And I remain on the login page
```

### Error Handling
```gherkin
Scenario: Network Error
  Given the API is unavailable
  When I try to [action]
  Then I see an error "Unable to connect. Please try again."
  And a "Retry" button is shown

Scenario: Validation Error
  Given I am on the [form] page
  When I submit with [invalid data]
  Then I see validation errors on the specific fields
  And the form is NOT submitted
```

## INVEST Checklist

- [ ] **I**ndependent — Can be developed and tested in isolation
- [ ] **N**egotiable — Details can be discussed with stakeholders
- [ ] **V**aluable — Delivers value to the end user
- [ ] **E**stimable — Team can estimate the effort
- [ ] **S**mall — Fits within a single sprint
- [ ] **T**estable — Has clear, verifiable acceptance criteria
