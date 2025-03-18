# Relatable: Connect your data!

Relatable (rltbl) is a tool for cleaning and connecting your data. It preserves invalid data while you're in the process of cleaning it, and it helps you connect your data to controlled terminologies and data standards. You can use Relatable as a web app, as a command-line tool, in a notebook like Jupyter, or from your favourite programming language.

This early version of Relatable has a minimal feature set:

- read from a SQLite database at `.relatable/relatable.db`
- look for a 'table' table with a column called 'table'
- get text for any table listed in 'table' (and *only* those tables)

The SQLite database could be one you created, or it could have been created by [VALVE](https://github.com/ontodev/valve.rs).

## Install

Use [Cargo](https://dev-doc.rust-lang.org/stable/cargo/index.html), the Rust package manager, to build Relatable. You can either:

1. use `cargo run` to compile and run a command, or
2. use `cargo build` to compile an executable `target/debug/rltbl`

## Usage

Run `rltbl help` to get a full list of commands and options. For example:

- `rltbl get table <TABLE>` prints the named table
- `rltbl get value <TABLE> <ROW> <COLUMN>` prints a single value

### Further reading:

- [Editing your data](doc/history.md)
- [Adding and removing messages](doc/message.md)
- [Tracking **rltbl** changes using Git](doc/git.md)

## Contributing

Contributions, issues, and feature requests are welcome!
Please read our [contribution guidelines](CONTRIBUTING.md)
and [code of conduct](CODE_OF_CONDUCT.md).

# License

Copyright © 2024 [Knocean, Inc.](https://knocean.com)
This project is [MIT](LICENSE) licensed.
