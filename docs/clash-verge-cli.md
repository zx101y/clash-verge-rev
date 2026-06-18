# Clash Verge Rev CLI

`clash-verge-cli` controls the running Clash Verge Rev GUI backend through an
authenticated loopback bridge. Read-only configuration commands can still work
when the GUI is not running, but runtime and system operations require the app.

## Common Commands

```text
clash-verge-cli --json status
clash-verge-cli core restart
clash-verge-cli core mode global
clash-verge-cli setting set enable_system_proxy true
clash-verge-cli profile switch "My Profile"
clash-verge-cli proxy groups --json
clash-verge-cli proxy select GLOBAL DIRECT
clash-verge-cli connection list --json
clash-verge-cli backup create
clash-verge-cli backup list --json
clash-verge-cli network interfaces --json
```

Run `clash-verge-cli help` for the complete command list.

## Destructive Operations

Profile deletion, backup restore/deletion, service changes, WebDAV
restore/deletion, and closing connections require `--yes`:

```text
clash-verge-cli profile delete old-profile --yes
clash-verge-cli backup restore backup.zip --yes
clash-verge-cli service repair --yes
```

## WebDAV Passwords

WebDAV passwords are read from an environment variable instead of a command
line argument:

```powershell
$env:CVR_DAV_PASSWORD = "secret"
clash-verge-cli webdav config https://dav.example user CVR_DAV_PASSWORD
```

The bridge listens only on `127.0.0.1` and authenticates requests with the
random token stored in the application data directory as `.cli-token`.
