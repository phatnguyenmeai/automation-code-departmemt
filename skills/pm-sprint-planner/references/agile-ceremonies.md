# Agile Ceremonies Reference

## Sprint Planning

**Duration**: 2 hours per 2-week sprint
**Participants**: PM, Dev, Frontend, Test
**Inputs**: Product backlog, team velocity, capacity
**Outputs**: Sprint goal, sprint backlog, story assignments

### Agenda
1. Review sprint goal (10 min)
2. Walk through prioritized stories (30 min)
3. Estimate and size stories (30 min)
4. Identify dependencies and risks (20 min)
5. Commit to sprint scope (15 min)
6. Break stories into tasks (15 min)

## Daily Standup

**Duration**: 15 minutes
**Format**: Each agent reports:
- What did I complete since last standup?
- What am I working on next?
- Any blockers?

## Sprint Review / Demo

**Duration**: 1 hour
**Agenda**:
1. Sprint goal recap (5 min)
2. Demo completed features (30 min)
3. Metrics review (10 min)
4. Stakeholder feedback (15 min)

## Sprint Retrospective

**Duration**: 1 hour
**Format**: Start / Stop / Continue
- **Start**: New practices to adopt
- **Stop**: Practices that aren't working
- **Continue**: Practices that work well

## Story Point Scale (Fibonacci)

| Points | Complexity | Example |
|--------|-----------|---------|
| 1 | Trivial | Fix a typo, update a config |
| 2 | Simple | Add a new field to an API |
| 3 | Moderate | New CRUD endpoint |
| 5 | Complex | New feature with UI + API |
| 8 | Very Complex | Multi-service integration |
| 13 | Epic-sized | Should be decomposed |

## Velocity Tracking

```
Sprint | Planned | Completed | Velocity
-------|---------|-----------|--------
S01    |    40   |    35     |   35
S02    |    38   |    36     |   36
S03    |    37   |    37     |   37
                    Avg Velocity: 36
```
