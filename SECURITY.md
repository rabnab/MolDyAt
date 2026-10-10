# Security Policy

## Supported versions

MolDyAt is a small research and teaching project. Only the latest release
receives fixes.

## Reporting a vulnerability

Please do not open a public issue for a security problem. Use GitHub's private
reporting instead: open the **Security** tab of this repository and choose
**Report a vulnerability**. Include the version (`moldyat --version`), your
platform, and the steps or the input file that triggers the problem.

Reports are handled on a best-effort basis by the maintainer. A fix is released
as a new version, and the reporter is credited unless they prefer otherwise.

## What counts as a vulnerability

MolDyAt runs locally and has no network features. Relevant reports are, for
example, crashes or unexpected behaviour when loading a crafted world file
(`--load`, Ctrl+O) or when parsing command-line options, and vulnerable
dependencies that are reachable from the program.

Ordinary bugs, including wrong physics results, belong in the normal issue
tracker.
