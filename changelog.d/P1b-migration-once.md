# P1b-migration-once

- Legacy Balanced cadence migration now runs once per `settings.json` (`schemaVersion` 1 -> 2), so setting 15 s / 10 min manually in 0.2.1+ sticks across restarts while staying compatible with 0.2.0.
