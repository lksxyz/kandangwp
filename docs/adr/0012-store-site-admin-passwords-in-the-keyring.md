# Site admin passwords live in the desktop keyring

A new site gets a random WordPress admin password. Kandang shows it once at
creation. If the user forgets it, the password would be lost because WordPress
stores only a hash.

**Decision: Kandang stores the password in the desktop keyring.** Provisioning
writes it through `secret-tool` with `service=kandang key=site-admin-<name>`.
The Overview tab has **Show password**, which asks for confirmation and then
fetches the password through a token-protected POST. The password hides again
after 30 seconds. **Copy** clears the clipboard after 30 seconds, but only if
the clipboard still holds that password; if the browser cannot read the
clipboard, Kandang leaves it alone. **Reset password** runs
`wp user update admin --user_pass` and replaces the keyring entry. Purging a
site clears the entry; moving it to the trash keeps the entry so a restore
still works.

The create dialog reports whether the keyring write succeeded. If `secret-tool`
or the Secret Service is unavailable, the password is shown once and the
dialog says so.

Alternatives considered:

- **Store the password in `settings.json` or the WordPress database.** Rejected:
  plain text near the site and visible to anything that can read the site
  directory.
- **Only offer a reset.** Lost passwords would always require a reset, which
  breaks existing admin sessions and any external integration. A keyring entry
  makes recall possible without changing the site.
- **Write the password into the create response only.** The current behavior
  before this decision. It forces the user to save the password at creation.

Consequences: the keyring must be unlocked to reveal the password, and anyone
who can run as the user can read it. That matches the local desktop trust level
and the Cloudflare sign-in session in the keyring (ADR 0017). The dashboard never
writes the password to a log or a site file.
