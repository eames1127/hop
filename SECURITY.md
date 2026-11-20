# Security policy

Hop reads and writes your SSH config, starts `ssh` and `ssh-keygen`, and can add public keys to
`authorized_keys` on your hosts, so security reports are very welcome.

## Supported versions

Only the latest release gets security fixes. Please check that a problem still happens there
before reporting it.

## Reporting a vulnerability

Please do not open a public issue for a security problem.

Report it privately through GitHub instead: on the repository's **Security** tab, choose
**Report a vulnerability**. Only the maintainer can see the report.

It helps to include:

- what an attacker could do, and what they would need first (for example, a crafted notes file
  or SSH config)
- steps to reproduce it, and the Hop version and operating system
- any ideas you have for a fix

You should get a reply within a week. Once a fix is released, you will be credited in the
release notes unless you would rather not be.

## What is in scope

- Running commands or changing files through anything Hop reads: the SSH config, imported notes
  files, pasted public keys or settings
- Hop writing something unintended to the SSH config or `authorized_keys`
- The window being able to do more than the commands it is given
- Hop's release builds and workflows

Problems in OpenSSH, your terminal or the operating system belong with those projects, though
it is still useful to hear if Hop makes one easier to reach.
