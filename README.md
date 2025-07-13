# onepass

Onepass is a CLI tool that may be used as a deterministic password generator. Think of it like a password manager (a la 1Password or the macOS Passwords app) except that instead of needing to store every password it generates, you only need to keep track of one seed password from which the rest can always be regenerated.

Onepass takes a single seed password and uses it to generate any number of per-site passwords. Individual site passwords can be cycled without changing all of your passwords by increasing an “increment” parameter for that site. Different password schemas are supported per site, such as “one lowercase letter, one uppercase letter, one digit, 15 alphanumeric characters.” The specification for these schemas is a regex-like syntax, and schemas may also be mapped to human-readable aliases; the [example config](example/config.yaml) demonstrates how these settings may be configured.

Optionally, you may sync your seed password to the system keyring. This protects it from being shoulder-surfed as you’re entering it, and on some platforms, is guarded by a user presence check (e.g. TouchID.)

Keyring sync can be requested either with the `-k` / `--keyring` CLI arg, or the `use_keyring: true` config setting.

## Installation

### GitHub releases

Download the [latest GitHub release](https://github.com/mrdomino/onepass/releases/latest). The macOS binaries (and the `onepass.pkg` macOS installer) are codesigned. All release artifacts are [attested](https://github.blog/news-insights/product-news/introducing-artifact-attestations-now-in-public-beta/), and these attestations can be verified, e.g. from the GitHub CLI:

```sh
gh attestation verify /path/to/onepass --owner mrdomino
```

### From cargo

On non-macOS platforms, onepass may be installed via cargo:

```sh
cargo install onepass
```

### From source

On non-macOS platforms, simply do:

```sh
cargo build --release &&
  sudo install target/release/onepass /usr/local/bin/onepass
```

Building from source on macOS requires codesigning, which probably requires you to edit [`onepass.entitlements`](onepass.entitlements) to replace the team ID with your own. See the [release GitHub workflow](.github/workflows/release.yaml) for an example of the steps required to build `onepass.pkg`, the macOS installer.

Assuming you’re using your Apple Development signing key, you should be able to do something like the following:

```sh
sed "s/2TM4K8523U.org.whilezero.app.onepass/$MY_TEAM_ID.*/" \
    onepass.entitlements > my-onepass.entitlements &&
  cargo build --release &&
  codesign \
    --force \
    --options runtime \
    --entitlements my-onepass.entitlements \
    --sign "Apple Development" \
    target/release/onepass
```

If it worked, you should be able to run `target/release/onepass -k google.com` and the command should succeed after reading your seed password; on a second run, or if there is already a password saved, you should see a TouchID prompt and not need to reenter your seed password.

## Quick start

Simply call `onepass` with the URL or hostname of the site for which you’d like to generate a password:

```sh
onepass google.com
```

You can override site settings with either config file entries or command-line flags. E.g. to use a password schema consisting of 18 alphanumeric characters:

```sh
onepass google.com -s '[A-Za-z0-9]{16}'
```

A default config file is generated at `${XDG_CONFIG_DIR:-$HOME/.config}/onepass/config.yaml` on the app’s first run. You may customize this config file; as it does not contain any sensitive data, you can back it up with other non-sensitive documents. (For some people, even giving a list of sites on which they have accounts is too sensitive; one day the app may support obfuscating site names. For now, use your best judgement.)

## How it works

At a high level, we take your seed password, turn it into a random number, and use that random number to select a password from the set of all possible passwords that could satisfy your site’s schema. Since the password stays the same across runs, the random numbers also stay the same, so you get the same site passwords every time without ever having to store any of them.

If you are interested in the details of how we do this, read on.

First we take your password schema and count the number of possible passwords that it supports; e.g. the schema `[0-9]` has 10 possible passwords, `[A-Z]` has 26 possible passwords, and `[0-9]{4}` has 10000 possible passwords. (The `-v` flag can tell you how many passwords a given schema supports, and approximately how many bits of entropy this provides; depending on your requirements, you probably want somewhere between 64 and 128 bits of entropy.)

Next we use a deterministic transform, described below, to turn your seed password into a CSPRNG (cryptographically secure pseudorandom number generator) that we sample to select a random number somewhere between 0 and the maximum number of passwords supported by your schema. Then we simply generate the password corresponding to that number and return it.

### Transform

We use the Argon2id key derivation function with your seed password as the key and the site configuration (specifically the increment number and URL) as salt. This gives us 32 bytes of key material, which we use to seed a ChaCha20 stream cipher that acts as our CSPRNG. We sample this CSPRNG’s output to choose a random 256-bit bigint less than the total number of passwords matching your schema. (If you need passwords with more than 256 bits of entropy, this is not the app for you.)

## Support me

This is a side project about which I am considering becoming more ambitious. To signal interest in me investing further effort into this, consider doing one or more of the following:

* Star the repo
* Tip me: <https://ko-fi.com/mrdomino>

## Acknowledgements

I initially came across deterministic password generators in 2013 via [habnabit/passacre](https://github.com/habnabit/passacre), which I happily used for several years until macOS had drifted to the point of making it prohibitively difficult to install. I then transitioned to [lesspass](https://lesspass.com/).

The idea for the regex-like schema syntax was due to [xfbs/passgen](https://github.com/xfbs/passgen).
