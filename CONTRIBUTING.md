# Contributing

Thanks for your interest in contributing!

## End-to-End Test

This repository includes an opt-in E2E test validating the full flow (generation → relative import rewriting).

```bash
# prerequisites: grpcio-tools available to your python_exe (python3 or uv)
E2E_RUN=1 cargo test --test e2e_smoke
```

## Dev Workflow

- Run formatting and lint before committing:
  ```bash
  makers format
  makers lint
  ```
- Build and run tests:
  ```bash
  makers build
  makers test
  ```

## Commit style

- Keep messages concise and focused on intent. Use conventional prefixes where appropriate (feat, fix, docs, test, refactor, chore).
