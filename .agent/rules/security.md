---
trigger: always_on
---

All features must assume:

- production databases
- real user data
- high cost of mistakes

Never:

- expose credentials to the frontend
- log sensitive data
- auto-run destructive queries

Always prefer:

- read-only by default
- confirmation for dangerous actions
- clear environment separation
