# Installation

RCL is written in Rust and builds with [Cargo][cargo]. RCL has few dependencies,
so it’s quick and easy to build from source, but you can also use one of the
options below that automate the process.

[cargo]: https://doc.rust-lang.org/cargo/guide/

## As a Nix flake

The repository includes a Nix flake. You can run <abbr>RCL</abbr> with a
[flake-enabled][flakes] version of [Nix][nix], such as Nix 2.18:

    nix run 'github:ruuda/rcl?ref=v0.9.0' -- --help

[flakes]: https://nixos.org/manual/nix/stable/command-ref/new-cli/nix3-flake
[nix]:    https://nixos.org/download

The Nix flake also includes the Python module:

    PYTHONPATH=$(nix build github:ruuda/rcl?ref=v0.9.0#pyrcl --print-out-paths)/lib python3

The Nix flake also includes a shell with all the tools needed for development,
as well as the environment that is tested on <abbr>CI</abbr>.

## From source

To build from source, clone the repository from one of the two mirrors:

    git clone https://github.com/ruuda/rcl.git
    git clone https://codeberg.org/ruuda/rcl.git

Then build with [Cargo][cargo]. The repository includes a `rust-toolchain.toml`
file that specifies a compatible Rust version. When Cargo is managed by
[Rustup][rustup], Rustup will automatically fetch the right toolchain.

    cargo build --release

Put the binary on your `PATH` to be able to use it system-wide, e.g.:

    cp target/release/rcl ~/.local/bin

To build a static binary rather than a dynamically linked one:

    cargo build --release --target x86_64-unknown-linux-musl
    cp target/x86_64-unknown-linux-musl/release/rcl ~/.local/bin

[cargo]:  https://doc.rust-lang.org/cargo/guide/
[rustup]: https://rust-lang.github.io/rustup/index.html

## Python module from source

To build the Python module, follow the steps as before, but build the `pyrcl`
directory:

    cargo build --release --manifest-path pyrcl/Cargo.toml

Then rename `libpyrcl.so` to `rcl.so` so that Python can discover it, and copy
it to a location on the `PYTHONPATH`, e.g.:

    cp target/release/libpyrcl.so ./rcl.so

Now you can use the module as any regular one:

    $ python3
    >>> import rcl
    >>> rcl.loads("10 + 32")
    42

It is also possible to build a wheel that can be installed into a virtualenv
using [Maturin](https://www.maturin.rs/).
