# HTTP Status Codes Quick Reference

## Success (2xx)
| Code | Name | Use |
|------|------|-----|
| 200 | OK | Successful GET/PUT/PATCH/DELETE |
| 201 | Created | Successful POST (resource created) |
| 204 | No Content | Successful DELETE (no body) |

## Client Errors (4xx)
| Code | Name | Use |
|------|------|-----|
| 400 | Bad Request | Validation errors, malformed JSON |
| 401 | Unauthorized | Missing/invalid authentication |
| 403 | Forbidden | Authenticated but not authorized |
| 404 | Not Found | Resource doesn't exist |
| 409 | Conflict | Duplicate resource (e.g., unique email) |
| 422 | Unprocessable Entity | Semantic validation failure |
| 429 | Too Many Requests | Rate limit exceeded |

## Server Errors (5xx)
| Code | Name | Use |
|------|------|-----|
| 500 | Internal Server Error | Unexpected server failure |
| 502 | Bad Gateway | Upstream service unavailable |
| 503 | Service Unavailable | Server overloaded/maintenance |
| 504 | Gateway Timeout | Upstream timeout |
