# Patched xattr 0.2.3

Upstream: https://github.com/Stebalien/xattr (MIT OR Apache-2.0)

`gpui_http_client` depends on `zed-async-tar`, which depends on `xattr 0.2.3`.
That release references `libc::ENOATTR` on Linux, but recent `libc` releases
no longer expose `ENOATTR` for linux-gnu (glibc never defined it), so the
build fails.

Patch: in `src/sys/mod.rs`, on `android`/`linux` define
`ENOATTR: c_int = 61` (the kernel's `ENODATA`, matching musl's `ENOATTR`).
Other platforms keep using `libc::ENOATTR`.

If upstream (or zed-async-tar) releases a fix, remove this patch and the
`[patch.crates-io]` section in the workspace `Cargo.toml`.
