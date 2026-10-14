# Repository instructions

These requirements apply to every change in this repository, including code,
documentation, configuration, and build or release automation.

## Version every change

- Every change set must include a new version bump. Use exactly `X.Y.Z`
  (major.minor.patch), with numeric components and no suffix.
- Increment the patch for fixes, documentation, and maintenance; the minor for
  backward-compatible features; and the major for breaking changes. One coherent
  change set needs one bump, not a bump for every intermediate edit.
- Treat `Cargo.toml` as the version source of truth. Keep `Cargo.lock`, Windows
  manifest/resource metadata, installer defaults, and displayed application
  versions consistent. Windows four-component metadata uses `X.Y.Z.0`.
- Never reuse a published version.

## Changelog

- Maintain `CHANGELOG.md` with a dated entry for every version, newest first.
- Describe the user-visible changes, fixes, and relevant upgrade notes for that
  version. Documentation and release-process changes must also be recorded.
- Include the version's changelog entry in its GitHub Release notes and distribute
  the changelog with the application and source package.

## Release from main using Git tags

- Release versions using tags named `vX.Y.Z` on commits in `main`.
- Before tagging, ensure the commit is on `main`, required checks pass, and the
  tag, package version, installer version, and changelog entry agree.
- Do not move or overwrite published release tags.

## Publish installable GitHub Releases

- The release pipeline must run for version tags, verify that the tagged commit
  belongs to `main`, and build and test that exact commit.
- Publish a GitHub Release for the tag in `alexpacio/robocam`. Attach the Windows
  installer as `robocam-X.Y.Z-windows-x64-setup.exe`, together with checksums and
  the corresponding source package. Publishing only an Actions artifact does not
  satisfy this requirement: the installer must be downloadable from the
  repository's **Releases** tab.
- Publish stable releases only after their assets are ready, so update checks do
  not offer an incomplete release.

## Keep automatic updates connected to releases

- Robocam's **Check for updates** functionality must discover releases from
  `https://api.github.com/repos/alexpacio/robocam/releases/latest` and compare
  versions semantically against the running application version.
- A newer stable release must be available for automatic download and deployment
  through its published installer; opening the Releases page alone does not
  satisfy the automatic-update requirement.
- Keep the pipeline's asset names and the updater's installer selection in sync.
  Ignore drafts, prereleases, and versions that are not newer.
- Validate downloaded installers against the published checksums over HTTPS.
  Preserve profiles and saved settings during installation, release held input
  before shutdown, and handle installation failure and restart cleanly.
- Validate the complete path when changing release or update behavior: tag on
  `main`, published installer, update discovery, download, installation, and
  restart with settings retained.

These are required repository practices. Do not claim a release or automatic
update has succeeded without verifying it; report implementation gaps explicitly.
