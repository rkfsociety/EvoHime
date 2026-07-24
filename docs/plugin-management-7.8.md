# Plugin Management (Stage 7.8)

## Overview

Stage 7.8 implements deterministic plugin versioning and soft-delete uninstall support:

- **Pin to commit**: lock plugin to specific git commit SHA, enabling reproducible installs
- **Pin to version**: semantic version pinning for future enhancement
- **Soft-delete uninstall**: mark plugin as uninstalled while preserving history for restore
- **Operator-scoped metadata**: each operator maintains independent plugin state

## API Endpoints

### Install Plugin
```
POST /api/plugins/install
Content-Type: application/json

{
  "name": "superpowers",
  "force": false
}
```

Response: `InstalledPlugin` with name, version, skills, risk_findings.

Database: Records entry in `installed_plugins` table with status='active'.

### Update Plugin (re-install)
```
POST /api/plugins/update
Content-Type: application/json

{
  "name": "superpowers",
  "force": false
}
```

Same as install, but with `replace_existing=true`. Re-activates if previously uninstalled.

### Uninstall Plugin (soft-delete)
```
POST /api/plugins/uninstall
Content-Type: application/json

{
  "name": "superpowers"
}
```

Response: HTTP 204 No Content.

Database: Soft-delete with status='uninstalled' and uninstalled_at timestamp.
Filesystem: Deletes plugin directory and removes from lock file.

### Pin Plugin to Commit/Version
```
POST /api/plugins/pin
Content-Type: application/json

{
  "name": "superpowers",
  "commit": "abc123def456...",
  "version": "1.2.3"
}
```

Response:
```json
{
  "status": "pinned",
  "name": "superpowers",
  "commit": "abc123def456...",
  "version": "1.2.3"
}
```

Database: Updates `pinned_commit` and `pinned_version` in DB, re-activates if uninstalled.

### List Installed Plugins
```
GET /api/plugins
```

Response: Array of `InstalledPlugin` objects with all active plugins.

### Plugin Integrity Check
```
GET /api/plugins/integrity
```

Response:
```json
{
  "lock_corrupted": false,
  "plugins": [
    {
      "name": "superpowers",
      "status": "ok",
      "locked_hash": "abc123...",
      "current_hash": "abc123...",
      "trust_level": "official",
      "installed_at": "2026-07-24T..."
    }
  ]
}
```

## Database Schema

### Table: `installed_plugins`

```sql
CREATE TABLE installed_plugins (
    id UUID PRIMARY KEY,
    operator_id UUID NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    pinned_commit TEXT,           -- git commit SHA (7.8)
    pinned_version TEXT,          -- semantic version (7.8 future)
    signature_hash TEXT,          -- Ed25519 signature (7.8 future)
    status TEXT NOT NULL,         -- 'active' | 'uninstalled' | 'quarantined'
    installed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ,
    uninstalled_at TIMESTAMPTZ,   -- soft-delete timestamp
    UNIQUE(operator_id, name)
);
```

## Lock File Format

`.evohime/plugins.lock.json` now includes pin information:

```json
{
  "superpowers": {
    "name": "superpowers",
    "version": "1.0.0",
    "source_url": "https://github.com/anthropics/superpowers.git",
    "git_ref": "v1.0.0",
    "content_hash": "abc123...",
    "trust_level": "official",
    "installed_at": "2026-07-24T...",
    "pinned_commit": "def456...",
    "pinned_version": "1.0.0"
  }
}
```

Fields `pinned_commit` and `pinned_version` are omitted if not set (backward compatible).

## Workflow: Install → Pin → Uninstall → Restore

### 1. Initial Install
```bash
curl -X POST http://localhost:3000/api/plugins/install \
  -H "Content-Type: application/json" \
  -d '{"name": "superpowers"}'
```

- Clones from catalog
- Runs risk scan, records in lock file with git_ref from catalog
- Records in DB with status='active', pins=None

### 2. Pin to Specific Commit
```bash
curl -X POST http://localhost:3000/api/plugins/pin \
  -H "Content-Type: application/json" \
  -d {
    "name": "superpowers",
    "commit": "abc123def456..."
  }
```

- Updates DB: pinned_commit = "abc123def456..."
- Next restore will use this commit instead of floating tag

### 3. Uninstall
```bash
curl -X POST http://localhost:3000/api/plugins/uninstall \
  -H "Content-Type: application/json" \
  -d '{"name": "superpowers"}'
```

- Removes filesystem directory
- Sets DB status='uninstalled', records uninstalled_at
- Preserves metadata for restore

### 4. List Uninstalled (hidden by default)
```bash
curl http://localhost:3000/api/plugins
# Does NOT include superpowers (status != 'active')
```

### 5. Restore by Re-installing
```bash
curl -X POST http://localhost:3000/api/plugins/update \
  -H "Content-Type: application/json" \
  -d '{"name": "superpowers"}'
```

- Looks up in catalog
- Re-clones (respects pinned_commit if set in DB)
- Sets status='active', clears uninstalled_at

## Operator Isolation

Each operator maintains independent plugin metadata. Two operators can have the same plugin name with different pins:

```
Operator A: superpowers, pin=v1.0.0
Operator B: superpowers, pin=v2.0.0 (or no pin)
```

Uninstalling for one operator does not affect the other.

## Test Coverage

See `crates/server/tests/plugin_install_lifecycle.rs` for:
- Install and list
- Soft-delete uninstall
- Pin and re-activate
- Operator isolation
- Upsert behavior on reinstall

Run tests:
```bash
DATABASE_URL=postgres://... cargo test --test plugin_install_lifecycle -- --ignored
```

## Future Enhancements (7.8+)

- **Signature verification**: Ed25519 signatures for plugin content
- **Semantic version resolution**: pin to version ranges (^1.0.0, ~1.2.0)
- **Plugin marketplace integration**: trust scores + automatic updates
- **Quarantine status**: operator can quarantine plugins without deletion
