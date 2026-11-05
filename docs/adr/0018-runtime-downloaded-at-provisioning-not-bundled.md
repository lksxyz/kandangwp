# Runtime artifacts are downloaded at provisioning, not bundled

Status: accepted. Supersedes ADR 0005.

Kandang's AppImage ships only the Rust binary. The runtime — FrankenPHP,
WordPress, WP-CLI, and the SQLite plugin — lives in
`~/.local/share/kandang/runtime` and is downloaded on first provisioning.

**Decision.**

- One tested runtime version for all Sites. The single-version rule from
  ADR 0005 is kept; what changes is how it reaches the machine.
- Runtime artifacts are downloaded at provisioning from pinned URLs and
  verified against pinned SHA-256 hashes. A cached file whose hash no longer
  matches is re-downloaded, so an update reaches existing installs without a
  new app release.
- The FrankenPHP binary is upstream v1.13.4 packed with UPX 5.0.2 (`--best`)
  and hosted in this repository's releases, not taken from the upstream
  release asset. Packed size is about 52 MB against ~175 MB unpacked.

**Consequences.**

The AppImage stays small, and a runtime update is one uploaded asset plus one
pin change in `src/provision.rs`. First provisioning needs network access, and
the runtime hosting, packing, and pin are ours to maintain. The UPX packing
costs about 0.5 s per PHP process start against about 70 ms unpacked; serving
and CLI behavior are unchanged.
