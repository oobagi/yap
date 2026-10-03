# Electron Builder macOS signing patch

`app-builder-lib@26.8.1.patch` fixes the password supplied to macOS
`security set-key-partition-list`. The temporary keychain has a generated
password, separate from the password used to import the signing certificate.

Upstream issue: https://github.com/electron-userland/electron-builder/issues/10066
Upstream fix: https://github.com/electron-userland/electron-builder/pull/10101

Remove the patch when upgrading to a package that includes the fix. Keep
`pnpm run electron:test-signing` passing against the installed dependency;
the test mocks keychain commands and runs on both macOS and Windows.
