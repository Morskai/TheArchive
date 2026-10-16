# Windows installation identity

`installer.nsi` is based on the Tauri CLI 2.12.0 upstream template:
https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.0/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi

The upstream file is licensed MIT OR Apache-2.0. Its original SHA-256 is
`dabed59013b1d78b879a1a85bc7f2eed2993b33a9a90cdabe5946de3d3950597`.

Our scoped changes add `INSTALLIDENTITY` and use it for `UNINSTKEY` and
`MANUPRODUCTKEY`, check the old executable before replacing it, and retarget
legacy shortcuts only when their targets match this installation. The old identity remains `本地影视库预览`, while the visible
product name, default fresh-install folder, and main executable are `TheArchive`.
The application identifier and data directory remain `com.localfilmlibrary.preview`.

Existing install locations are read from the original registry key; upgrading
does not move a user's existing installation or reset application data. The
upstream installer handles the previous executable name using `MainBinaryName`.
New display names and generated-script checks do not constitute an installation
or upgrade test; isolated native installation remains separately recorded.

When updating Tauri, review and rebase this template against the corresponding
upstream version rather than treating it as a generated installer script.
