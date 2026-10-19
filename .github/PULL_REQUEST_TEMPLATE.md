## Summary

<!-- What does this PR do in 1-3 sentences? -->

## Type of change

<!-- Mark exactly one. There is no automation that applies labels to pull
     requests, so add the matching label by hand after opening this PR:
     bug fix -> `bug`, new feature -> `enhancement`, breaking change -> `enhancement`,
     documentation -> `documentation`, refactoring -> `ci`,
     maintenance/tooling -> `ci` (or none if it does not fit).
     Also add the area label: `backend`, `frontend` or `database`. -->

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update
- [ ] Refactoring (no functional change)
- [ ] Test improvement
- [ ] Maintenance / tooling (CI, dependencies, repository configuration)

## Checklist

<!-- An unverified box is worse than an empty one: check a box only when you
     actually ran that check. `make test` runs all three levels and needs only
     `docker compose up -d postgres`; it uses the test database, never your own.
     See docs/en/testing.md. -->

- [ ] My code follows the project's style
- [ ] I have added tests for my changes
- [ ] Existing tests pass locally (`make test`: unit + integration + e2e)
- [ ] If I changed the backend, the image still boots: `docker compose build backend` before shipping
- [ ] Linter passes locally (`make lint`)
- [ ] I have updated documentation if needed
- [ ] My commits follow [Conventional Commits](https://www.conventionalcommits.org/)
- [ ] I added a type label and an area label (`backend` / `frontend` / `database`)

## Screenshots (if UI change)

<!-- Add before/after screenshots if the change is visual -->

## Related issue

<!-- Fixes #123 -->
