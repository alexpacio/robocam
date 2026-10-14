# Supplemental dependency license texts

Packaging copies original license/notice files from every resolved Cargo crate,
including `AUTHORS` and `README` files containing licensing or attribution.
These version-specific supplements cover license texts omitted from crate archives:

- `rust/opencv-binding-generator-0.107.0/LICENSE` is copied verbatim from
  [the upstream repository at the crate's recorded revision](https://github.com/twistedfall/opencv-rust/blob/09d5441d97f0894f1a09e750a133684ff74e3aa3/LICENSE).
  The crate's `.cargo_vcs_info.json` records that revision and `binding-generator`
  subdirectory; the MIT license is in the repository root.
- `rust/winsplit-0.1.0/LICENSE-APACHE` is the unmodified
  [Apache-2.0 text](https://www.apache.org/licenses/LICENSE-2.0.txt), selected under
  [the version's upstream license declaration](https://github.com/chipsenkbeil/winsplit-rs/blob/691e800ac384bc7fc815ca55d1dd2b09b1f15d96/README.md#license).
  Both the crate archive and that revision omit standalone license files. Its
  original README and Cargo author metadata are retained in the package.

`r-efi` 6.0.0 includes its MIT permission notice, warranty disclaimer and author
attribution in `AUTHORS`; packaging preserves that file.

When updating dependencies, review these supplements against the new exact
versions. Packaging fails if any resolved dependency has no license notice.
