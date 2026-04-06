# MongoDB Schema Design Patterns

## Pattern Selection Guide

| Pattern | Use When | Avoid When |
|---------|----------|------------|
| **Embedding** | Data is read together, 1:few, rarely changes | Data is large, 1:many, frequently updated independently |
| **Referencing** | Data is 1:many, updated independently | Data is always read together (N+1 risk) |
| **Bucket** | Time-series data, IoT, logs | Data needs random access by individual record |
| **Polymorphic** | Multiple entity types in one collection | Types have completely different access patterns |
| **Computed** | Frequently read aggregates, expensive computations | Data changes rapidly, consistency is critical |
| **Outlier** | Most docs are small, few are large | Uniform document sizes |

## Schema Versioning

```json
{
  "_id": "...",
  "_schema_version": 2,
  "name": "John",
  "email": "john@example.com"
}
```

```rust
fn migrate_user(doc: &mut Document) {
    match doc.get_i32("_schema_version").unwrap_or(1) {
        1 => {
            // v1 → v2: split "name" into "first_name" + "last_name"
            if let Some(name) = doc.get_str("name").ok() {
                let parts: Vec<&str> = name.splitn(2, ' ').collect();
                doc.insert("first_name", parts[0]);
                doc.insert("last_name", parts.get(1).unwrap_or(&""));
                doc.remove("name");
            }
            doc.insert("_schema_version", 2);
        }
        _ => {}
    }
}
```

## Index Design Rules (ESR)

1. **Equality** fields first (exact match: `status = "active"`)
2. **Sort** fields next (ordering: `created_at: -1`)
3. **Range** fields last (comparison: `total > 100`)

```javascript
// Query: find active orders sorted by date where total > 100
// Index: { status: 1, created_at: -1, total: 1 }
db.orders.find({ status: "active", total: { $gt: 100 } }).sort({ created_at: -1 })
```

## Aggregation Pipeline Optimization

```javascript
// BAD: $lookup then $match (processes all documents)
[
  { $lookup: { from: "orders", ... } },
  { $match: { "orders.status": "active" } }
]

// GOOD: $match first to reduce documents before $lookup
[
  { $match: { active: true } },
  { $lookup: { from: "orders", ... } },
  { $unwind: "$orders" },
  { $match: { "orders.status": "active" } }
]
```
