# Security

yesyesyes can allow commands to run without the user seeing them, so security reports matter.

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's private report:
<https://github.com/jacopoRufini/yesyesyes/security/advisories/new>

Include what you found, how to reproduce it and the version. You can expect an answer within a
week. Fixes are released as a new version and credited to you if you wish.

## In scope

- A request that is allowed when the rules say it must be asked (for example a way past the
  chained-command check of the regex and wildcard modes).
- Another program, web page or user making yesyesyes answer or leak requests (the hook server
  listens on 127.0.0.1:47331 only).
- Secrets or request text written somewhere they should not be.

## Known limits (documented in the README)

- **Follow Claude's own opinion** is an experiment, not a safeguard.
- Any program can listen on port 47331 while yesyesyes is not running.
- The release binaries are not signed or notarized.
