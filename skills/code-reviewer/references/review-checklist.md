# Code Review Checklist

## Correctness
- [ ] Logic is correct and handles all expected inputs
- [ ] Edge cases are handled (empty, null, max values)
- [ ] Error paths are handled properly
- [ ] No off-by-one errors in loops/ranges
- [ ] Async/await is used correctly (no missing awaits)

## Security
- [ ] No SQL/NoSQL injection vulnerabilities
- [ ] No XSS in rendered content
- [ ] No secrets/credentials in code
- [ ] Input validation at system boundaries
- [ ] Authentication/authorization checks in place
- [ ] No SSRF vulnerabilities in URL handling

## Performance
- [ ] No N+1 query patterns
- [ ] No unnecessary allocations in hot paths
- [ ] Database queries use indexes
- [ ] No unbounded collection growth
- [ ] Pagination on list endpoints

## Rust-Specific
- [ ] No `.unwrap()` in production paths
- [ ] Proper error types (thiserror/anyhow)
- [ ] Clippy passes with zero warnings
- [ ] No unnecessary clones
- [ ] Lifetimes are correct

## Style
- [ ] Naming follows conventions (snake_case for Rust, camelCase for JS/TS)
- [ ] No dead code or commented-out blocks
- [ ] Functions are focused (single responsibility)
- [ ] Complex logic has brief comments explaining "why"
