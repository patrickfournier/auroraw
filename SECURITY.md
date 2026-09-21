# Security policy

## Reporting a vulnerability

Please report security problems **privately**, through GitHub's private vulnerability reporting:
open the **Security** tab of this repository and choose **Report a vulnerability**. Please do
not open a public issue for a vulnerability.

Include what you found, how to reproduce it (a crafted file is the best evidence), the version or
commit, and the platform.

## What is in scope

- An escape from the plugin sandbox, or a plugin reading or writing what it was not granted.
- A crash, hang or code execution triggered by a crafted sidecar, state file, RAW file, plugin
  package or plugin index entry.
- A way to make Auroraw send data over the network without the user's consent.
- A flaw in the update check, the plugin index or the signing of releases.

## What to expect

- An acknowledgement within **7 days**.
- A fix or a mitigation plan within **90 days**, and a coordinated disclosure once a fix ships.
- Credit in the release notes if you want it.

## Supported versions

The latest release. While the project is young, the previous minor release as well.
There is no released version yet: Auroraw is in its design phase (see
[docs/](docs/)).
