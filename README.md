# onepass

This is my CLI password generator. There are many like it, but this one is mine.

More specifically, this is a deterministic password manager: turn one master password into any number of unique, strong per-site passwords. No secrets stored, config can be public.

**Caveat emptor:** prior to v1, I reserve the right to make changes that result in different passwords getting generated for the same site and master password, and will endeavor to bump the minor version number when I do this. After v1, I will only make such changes if absolutely necessary — e.g. if a flaw is found in the cryptosystem — and these will be accompanied by a new major version.

## Installation

### macOS (Recommended)
Download signed binaries from [GitHub releases](https://github.com/mrdomino/onepass/releases/latest).

### Cargo
```sh
cargo install onepass
```

### From Source
```sh
cargo build --release && install target/release/onepass ~/bin/onepass
```

A default config is generated at `${XDG_CONFIG_DIR:-$HOME/.config}/onepass/config.yaml` on first run. See also the included [example config](example/config.yaml).

## Quick Start

```sh
onepass google.com
# Enter master password when prompted
# → Generates deterministic password for https://google.com/
```

Use schemas to control password format:
```sh
onepass -s pin iphone.local    # 8-digit PIN
onepass -s phrase github.com   # 5-word passphrase
```

## Key Features

**URL Canonicalization**: `google.com` becomes `https://google.com/` — same master password always generates the same site password.

**Schema Support**: Regex-like patterns control password format:
- `[A-Za-z0-9]{18}` — 18 alphanumeric characters
- `[:word:](-[:word:]){4}` — 5 words separated by dashes
- `[!-~]{12}` — 12 printable ASCII characters (default)

**Password Rotation**: Increment parameter lets you rotate passwords without changing your master password.

**Usernames**: Allows you to use different passwords for different accounts on a site.

## How It Works

We use Argon2id to derive a 256-bit key from your master password with salt `{increment},{url}`. That key seeds a BLAKE3 extendable output function, which generates a uniform random number to select from all possible passwords matching your schema.

Same inputs → same outputs. Always.

## Tips & Support

If you find this useful:
- ★ Star the repo
- [Buy me a taco](https://ko-fi.com/mrdomino) 🌮

For technical details, see [HACKING.md](HACKING.md).

## Acknowledgements

Inspired by [passacre](https://github.com/habnabit/passacre) and [lesspass](https://lesspass.com). Schema idea from [xfbs/passgen](https://github.com/xfbs/passgen). Crypto recommendations from Justine Tunney. Word list from [the EFF](https://www.eff.org/dice).
