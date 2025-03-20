# How to contribute

Relatable is an open source project that relies on people like you to make it better!

- report [issues](issues)
- suggest a [feature](issue)
- make a [pull request](pulls)
- suggest [changes to this page](edit/main/CONTRIBUTING.md) to make it more helpful!

Keep in mind that we have a [code of conduct](CODE_OF_CONDUCT.md) to help foster a welcoming community.

We'll do our best to reply to issues and pull requests, but please understand that we can't act on every one of them. If this project isn't suiting your needs, you're free to make your own fork, under our MIT license.

## Coding conventions

- use automatic formatters with no overrides, following the lead of [Helix](https://github.com/helix-editor/helix/blob/master/languages.toml)
  - [`rustfmt`](https://github.com/rust-lang/rustfmt) for Rust
  - [`ruff`](https://github.com/astral-sh/ruff) for Python
  - [`superhtml`](https://github.com/kristoff-it/superhtml) for HTML
  - [`shellcheck`](https://github.com/koalaman/shellcheck) for shell scripts

## Creating a new release using [Cargo-dist](https://opensource.axo.dev/cargo-dist/book/)

1. Add (if it isn't already present) a value for **repository** in the **[package]** section of
   your `Cargo.toml` file:

        [package]
        name = "rltbl"
        repository = "https://github.com/rltbl/relatable"
        ...

2. Install dist if it isn't already installed (see the [install guide](https://opensource.axo.dev/cargo-dist/book/install.html))

3. Verify that the configuration is (still) correct.

        dist init

    The effect of running `dist init` is to (re)generate the GitHub workflow file
    `.github/workflows/release.yml`, which should therefore not normally be manually edited.

4. Add a new tag to the last commit:

        git tag v0.1.0

5. Push the tag.

        git push --tags

6. Observe on GitHub that the workflow creates a new release corresponding to the tag in your repository.
