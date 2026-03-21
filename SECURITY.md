# Security Policy

## Supported Versions

Only the latest release on the `main` branch is supported with security updates. We do not backport fixes to older versions.

| Branch | Supported |
|--------|-----------|
| `main` | Yes |
| Other  | No  |

## Reporting a Vulnerability

**Please do not open a public issue for security vulnerabilities.**

Use GitHub's private vulnerability reporting to submit a report:

1. Go to the [Security tab](https://github.com/jasonherald/mac-doc-hyprland/security)
2. Click **"Report a vulnerability"**
3. Provide a description, steps to reproduce, and any relevant details

### What to expect

- **Acknowledgment** within 48 hours
- **Assessment** of severity and impact within 1 week
- **Fix or mitigation** as soon as practical, depending on severity
- Credit in the fix commit (unless you prefer to remain anonymous)

## Security Scanning

This project uses automated security scanning on every PR and weekly:

| Tool | Coverage |
|------|----------|
| [CodeQL](https://codeql.github.com/) | Source-level OWASP analysis (command injection, path traversal, tainted data flows) |
| [cargo-audit](https://rustsec.org/) | Known CVEs in Rust dependencies (RustSec advisory database) |
| [cargo-deny](https://embarkstudios.github.io/cargo-deny/) | License compliance, duplicate crates, source restrictions |
| [CodeRabbit](https://coderabbit.ai/) | AI-assisted code review with OSV dependency scanning |
| [SonarQube](https://www.sonarqube.org/) | Code quality, cognitive complexity, code smells |

## Scope

This project runs as a user-space application on Wayland compositors (Hyprland, Sway). It:

- Executes `.desktop` file `Exec=` commands via the compositor
- Reads/writes pin state to `~/.cache/mac-dock-pinned`
- Listens on D-Bus as a notification daemon (`org.freedesktop.Notifications`)
- Communicates with the compositor via IPC sockets

Vulnerabilities in any of these areas are in scope.
