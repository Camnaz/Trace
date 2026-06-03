# CI/CD & Stability Workflows

This directory contains GitHub Actions workflows for continuous integration, periodic testing, and stability management.

## Workflows

### 1. CI (ci.yml)

**Triggers:** Every push and PR to `main`/`master`

**Jobs:**
- **Test Suite**: Runs on Rust 1.75.0 and stable across all platforms
- **Lint & Format**: Enforces `cargo fmt` and `cargo clippy`
- **Code Coverage**: Generates coverage reports with tarpaulin
- **Security Audit**: Checks dependencies with `cargo-audit`

### 2. Periodic Stability Tests (periodic-tests.yml)

**Triggers:** Every 6 hours + manual dispatch

**Purpose:** Detects flakiness, performance regressions, and platform-specific issues

**Jobs:**
- **Full Test Suite**: Tests on Ubuntu/macOS with Rust 1.75.0/stable/nightly
- **Performance Benchmarks**: Measures execution time trends
- **Stability Check**: Runs tests 10x to detect flaky tests
- **Auto-notification**: Creates GitHub issue on failure

### 3. Tag Stable Version (stability-tag.yml)

**Triggers:** After successful CI on main/master

**Purpose:** Creates stable tags for easy reversion

**Features:**
- Creates `stable-latest` tag (moves with each stable commit)
- Creates dated tags: `stable-YYYYMMDD-SHA`
- Creates GitHub Release with rollback instructions

### 4. Rollback to Stable (rollback.yml)

**Triggers:** Manual workflow dispatch only

**Purpose:** Emergency reversion to last known good version

**Safety Features:**
- Requires `YES` confirmation
- Creates backup tag before rollback
- Creates GitHub issue documenting the rollback

## Stability Tags

After each successful CI run on main, a stable tag is created:

```
stable-latest          → Always points to latest passing commit
stable-20240602-abc123 → Specific dated stable version
```

## Quick Reference

### View Recent Stable Versions

```bash
git fetch --tags
git tag -l 'stable-*' | sort -r | head -10
```

### Revert to Last Stable Version

```bash
# Checkout stable version
git checkout stable-latest

# Or reset branch to stable
git checkout main
git reset --hard stable-latest
git push --force-with-lease
```

### Check Test History

View all test runs: **Actions** tab → **Periodic Stability Tests**

Download artifacts for analysis:
- `test-results-*` - Full test output
- `timing-results` - Performance benchmarks
- `stability-report` - Flakiness detection logs

## Artifacts Retention

- Test results: 30 days
- Performance/timing: 90 days
- Stability reports: 90 days

## Badge Status

Add this to your main README:

```markdown
![CI](https://github.com/OWNER/REPO/actions/workflows/ci.yml/badge.svg)
![Periodic Tests](https://github.com/OWNER/REPO/actions/workflows/periodic-tests.yml/badge.svg)
```
